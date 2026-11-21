//! Pure parsers for every upstream payload. Nothing here touches the network.
//!
//! The XML side uses a small hand-written tag scanner: enough to walk the two
//! EPD documents, deliberately tolerant about attributes, CDATA and stray
//! whitespace.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use crate::advice::AdviceRow;
use crate::error::{Error, Result};
use crate::model::{
    AQHI_STATIONS, AqhiReading, ForecastEntry, ForecastPeriod, HistoryReading, HkoReading,
    MonitorDetail, MonitorStation, ParsedForecast, ParsedPollutants, PollutantReading,
    PollutantRecord, StreetReading, WeatherReport, app_station_name,
};
use crate::weather::ForecastDay;

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

/// Strip a leading UTF-8 BOM and any leading whitespace.
///
/// The 24-hour pollutant XML is served with a BOM, which would otherwise end up
/// glued to the XML declaration.
#[must_use]
pub fn strip_bom(input: &str) -> &str {
    input.trim_start_matches('\u{feff}').trim_start()
}

/// Return the trimmed text of the first `<tag>` element inside `block`.
///
/// Attributes on the opening tag are skipped and `<![CDATA[...]]>` wrappers are
/// removed. Returns `None` when the element is absent or unterminated.
#[must_use]
pub fn inner_text<'a>(block: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}");
    let start = block.find(&open)?;
    let after_open = &block[start + open.len()..];
    let tag_end = after_open.find('>')?;
    let rest = &after_open[tag_end + 1..];
    let close = format!("</{tag}>");
    let end = rest.find(&close)?;
    Some(unwrap_cdata(&rest[..end]).trim())
}

/// Collect the inner text of every `<tag>...</tag>` element in `xml`.
#[must_use]
pub fn blocks<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut found = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = xml[cursor..].find(&open) {
        let start = cursor + offset;
        let Some(tag_end) = xml[start..].find('>') else {
            break;
        };
        let content_start = start + tag_end + 1;
        let Some(content_end) = xml[content_start..]
            .find(&close)
            .map(|at| content_start + at)
        else {
            break;
        };
        found.push(&xml[content_start..content_end]);
        cursor = content_end + close.len();
    }
    found
}

fn unwrap_cdata(raw: &str) -> &str {
    let trimmed = raw.trim();
    match trimmed
        .strip_prefix("<![CDATA[")
        .and_then(|inner| inner.strip_suffix("]]>"))
    {
        Some(inner) => inner.trim(),
        None => trimmed,
    }
}

/// Parse a measurement cell.
///
/// `-` (not measured at that station), `NA` and empty cells are `None`;
/// values written without a leading zero (`.8`) parse as `0.8`.
#[must_use]
pub fn parse_measurement(raw: &str) -> Option<f64> {
    let text = raw.trim();
    if text.is_empty()
        || text == "-"
        || text.eq_ignore_ascii_case("na")
        || text.eq_ignore_ascii_case("n/a")
        || text.eq_ignore_ascii_case("nd")
    {
        return None;
    }
    if let Ok(value) = text.parse::<f64>() {
        return Some(value);
    }
    let numeric: String = text
        .chars()
        .take_while(|character| {
            character.is_ascii_digit()
                || *character == '.'
                || *character == '-'
                || *character == '+'
        })
        .collect();
    numeric.parse::<f64>().ok()
}

/// Normalise a timestamp to ISO-8601, keeping the offset.
///
/// RFC 2822 style values (`Fri, 20 Nov 2026 18:00:00 +0800`) become
/// `2026-11-20T18:00:00+08:00`. Anything that cannot be understood is returned
/// trimmed but otherwise untouched.
#[must_use]
pub fn normalize_datetime(raw: &str) -> String {
    let text = raw.trim();
    if let Some(iso) = rfc2822_to_iso(text) {
        return iso;
    }
    if has_iso_date(text) {
        return text.to_string();
    }
    text.to_string()
}

/// Convert an RFC 2822 timestamp into ISO-8601 with a `+HH:MM` offset.
#[must_use]
pub fn rfc2822_to_iso(raw: &str) -> Option<String> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    let body = match text.split_once(',') {
        Some((head, rest))
            if head
                .chars()
                .all(|character| character.is_ascii_alphabetic()) =>
        {
            rest.trim()
        }
        _ => text,
    };
    let mut parts = body.split_whitespace();
    let day: u32 = parts.next()?.parse().ok()?;
    if !(1..=31).contains(&day) {
        return None;
    }
    let month = month_number(parts.next()?)?;
    let year: i32 = parts.next()?.parse().ok()?;
    let Some(time) = parts.next() else {
        return Some(format!("{year:04}-{month:02}-{day:02}"));
    };
    let (hour, minute, second) = parse_hms(time)?;
    let offset = parts.next().and_then(iso_offset)?;
    Some(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{offset}"
    ))
}

fn month_number(token: &str) -> Option<u32> {
    let key = token.trim().to_ascii_lowercase();
    MONTHS
        .iter()
        .position(|month| key.starts_with(month))
        .map(|index| index as u32 + 1)
}

fn parse_hms(token: &str) -> Option<(u32, u32, u32)> {
    let mut parts = token.split(':');
    let hour: u32 = parts.next()?.parse().ok()?;
    let minute: u32 = parts.next()?.parse().ok()?;
    let second: u32 = parts.next().unwrap_or("0").parse().ok()?;
    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    Some((hour, minute, second))
}

fn iso_offset(token: &str) -> Option<String> {
    let text = token.trim();
    match text {
        "Z" | "z" | "GMT" | "UTC" | "UT" => return Some("+00:00".to_string()),
        _ => {}
    }
    let mut characters = text.chars();
    let sign = characters.next()?;
    if sign != '+' && sign != '-' {
        return None;
    }
    let digits: String = characters.collect();
    if digits.len() == 4 && digits.chars().all(|character| character.is_ascii_digit()) {
        return Some(format!("{sign}{}:{}", &digits[..2], &digits[2..]));
    }
    if digits.len() == 2 && digits.chars().all(|character| character.is_ascii_digit()) {
        return Some(format!("{sign}{digits}:00"));
    }
    None
}

fn has_iso_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 10
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && bytes[4] == b'-'
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[7] == b'-'
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

/// Parse the AQHI per-station JSON array.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the payload is not an array of station objects
/// or contains no station readings.
pub fn parse_aqhi_json(input: &str) -> Result<Vec<AqhiReading>> {
    let root: Value = serde_json::from_str(strip_bom(input))?;
    let array = root
        .as_array()
        .ok_or_else(|| Error::parse("AQHI JSON: expected a top-level array"))?;
    let mut readings = Vec::new();
    for item in array {
        let Some(station) = value_text(item, "station") else {
            continue;
        };
        readings.push(AqhiReading {
            station,
            aqhi: value_i64(item, "aqhi"),
            health_risk: value_text(item, "health_risk"),
            publish_date: value_text(item, "publish_date").map(|raw| normalize_datetime(&raw)),
        });
    }
    if readings.is_empty() {
        return Err(Error::parse(
            "AQHI JSON: no station readings found in payload",
        ));
    }
    Ok(readings)
}

/// Parse the AQHI RSS fallback.
///
/// The format is not stable across releases, so each `<item>` is scanned for a
/// known station name, the first small integer (the AQHI band) and a risk
/// keyword. Title text is used as the station name when no known name appears.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the document has no usable `<item>` elements.
pub fn parse_aqhi_rss(input: &str) -> Result<Vec<AqhiReading>> {
    let xml = strip_bom(input);
    let items = blocks(xml, "item");
    if items.is_empty() {
        return Err(Error::parse("AQHI RSS: no <item> elements found"));
    }
    let mut readings = Vec::new();
    for item in items {
        let title = inner_text(item, "title").unwrap_or("");
        let description = inner_text(item, "description").unwrap_or("");
        // Only the title and description are scanned: a <pubDate> would donate
        // day-of-month and month integers that could be mistaken for an AQHI.
        let scanned = format!("{title} {description}");
        let station = detect_station(&scanned).unwrap_or_else(|| title.trim().to_string());
        if station.is_empty() {
            continue;
        }
        readings.push(AqhiReading {
            station,
            aqhi: first_small_integer(&scanned),
            health_risk: detect_risk(&scanned),
            publish_date: inner_text(item, "pubDate")
                .or_else(|| inner_text(item, "date"))
                .map(normalize_datetime),
        });
    }
    if readings.is_empty() {
        return Err(Error::parse("AQHI RSS: no readable station items"));
    }
    Ok(readings)
}

fn detect_station(text: &str) -> Option<String> {
    let haystack = text.to_lowercase();
    let mut best: Option<&str> = None;
    for station in AQHI_STATIONS {
        if haystack.contains(&station.to_lowercase())
            && best.is_none_or(|current| station.len() > current.len())
        {
            best = Some(station);
        }
    }
    best.map(str::to_string)
}

fn first_small_integer(text: &str) -> Option<i64> {
    let mut digits = String::new();
    for character in text.chars() {
        if character.is_ascii_digit() {
            digits.push(character);
            continue;
        }
        if let Some(value) = small_value(&digits) {
            return Some(value);
        }
        digits.clear();
    }
    small_value(&digits)
}

fn small_value(digits: &str) -> Option<i64> {
    if digits.is_empty() || digits.len() > 2 {
        return None;
    }
    let value: i64 = digits.parse().ok()?;
    // 1-10 are the numeric bands, 11 is the "Serious" band.
    if (1..=11).contains(&value) {
        Some(value)
    } else {
        None
    }
}

fn detect_risk(text: &str) -> Option<String> {
    const LEVELS: [(&str, &str); 5] = [
        ("very high", "Very High"),
        ("serious", "Serious"),
        ("high", "High"),
        ("moderate", "Moderate"),
        ("low", "Low"),
    ];
    let haystack = text.to_lowercase();
    LEVELS
        .iter()
        .find(|(needle, _)| haystack.contains(needle))
        .map(|(_, label)| (*label).to_string())
}

/// Parse the past-24-hour pollutant concentration XML.
///
/// # Errors
///
/// Returns [`Error::Parse`] when no `<PollutantConcentration>` block can be read.
pub fn parse_pollutants_xml(input: &str) -> Result<ParsedPollutants> {
    let xml = strip_bom(input);
    let last_build_date = inner_text(xml, "lastBuildDate").map(str::to_string);
    let mut readings = Vec::new();
    for block in blocks(xml, "PollutantConcentration") {
        let Some(station) = inner_text(block, "StationName") else {
            continue;
        };
        let station = station.trim();
        if station.is_empty() {
            continue;
        }
        let time = inner_text(block, "DateTime")
            .map(normalize_datetime)
            .unwrap_or_default();
        readings.push(PollutantRecord {
            station: station.to_string(),
            reading: PollutantReading {
                time,
                no2: measurement(block, "NO2"),
                o3: measurement(block, "O3"),
                so2: measurement(block, "SO2"),
                co: measurement(block, "CO"),
                pm10: measurement(block, "PM10"),
                pm25: measurement(block, "PM2.5"),
            },
        });
    }
    if readings.is_empty() {
        return Err(Error::parse(
            "24-hour pollutant XML: no <PollutantConcentration> blocks found",
        ));
    }
    Ok(ParsedPollutants {
        last_build_date,
        readings,
    })
}

fn measurement(block: &str, tag: &str) -> Option<f64> {
    inner_text(block, tag).and_then(parse_measurement)
}

/// Parse the HKO `rhrread` JSON.
///
/// Only `updateTime`, `temperature`, `humidity` and `warningMessage` are used.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the document is not an object or carries no
/// readings at all.
pub fn parse_weather_json(input: &str) -> Result<WeatherReport> {
    let root: Value = serde_json::from_str(strip_bom(input))?;
    if !root.is_object() {
        return Err(Error::parse("HKO JSON: expected a top-level object"));
    }
    let update_time = value_text(&root, "updateTime").map(|raw| normalize_datetime(&raw));
    let temperature = hko_readings(&root, "temperature");
    let humidity = hko_readings(&root, "humidity");
    let warnings = hko_warnings(&root);
    if update_time.is_none() && temperature.is_empty() && humidity.is_empty() {
        return Err(Error::parse("HKO JSON: no readings found in payload"));
    }
    Ok(WeatherReport {
        update_time,
        temperature,
        humidity,
        warnings,
        source: "HKO".to_string(),
    })
}

fn hko_readings(root: &Value, key: &str) -> Vec<HkoReading> {
    let Some(data) = root
        .get(key)
        .and_then(|value| value.get("data"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    data.iter()
        .filter_map(|item| {
            let place = value_text(item, "place")?;
            Some(HkoReading {
                place,
                value: value_f64(item, "value"),
                unit: value_text(item, "unit"),
            })
        })
        .collect()
}

fn hko_warnings(root: &Value) -> Vec<String> {
    let mut warnings = Vec::new();
    match root.get("warningMessage") {
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(text) = item.as_str() {
                    let cleaned = strip_tags(text);
                    if !cleaned.is_empty() {
                        warnings.push(cleaned);
                    }
                }
            }
        }
        Some(Value::String(text)) => {
            let cleaned = strip_tags(text);
            if !cleaned.is_empty() {
                warnings.push(cleaned);
            }
        }
        _ => {}
    }
    warnings
}

/// Newest timestamp embedded in a feed body, as Unix seconds.
///
/// `doctor` uses this to notice an upstream that keeps answering `200` with
/// frozen data: it reads `<lastBuildDate>`/`<pubDate>`, then the newest ISO-8601
/// instant, then compact `YYYYMMDDHH[MM]` stamps such as the HKO CSV files.
#[must_use]
pub fn latest_timestamp(body: &str) -> Option<i64> {
    let tagged = inner_text(body, "lastBuildDate")
        .or_else(|| inner_text(body, "pubDate"))
        .and_then(rfc2822_to_iso)
        .and_then(|iso| unix_from_iso(&iso));
    let scanned = scan_timestamps(body).max();
    match (tagged, scanned) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (tagged, scanned) => tagged.or(scanned),
    }
}

/// Every recognisable timestamp in `body`, as Unix seconds.
fn scan_timestamps(body: &str) -> impl Iterator<Item = i64> + '_ {
    let bytes = body.as_bytes();
    let mut found = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_digit() && (index == 0 || !bytes[index - 1].is_ascii_digit()) {
            let mut end = index;
            while end < bytes.len() && bytes[end].is_ascii_digit() {
                end += 1;
            }
            let run = &body[index..end];
            if let Some(unix) = compact_stamp(run) {
                found.push(unix);
            }
            index = end;
            continue;
        }
        index += 1;
    }
    // ISO instants: `YYYY-MM-DDTHH:MM`.
    let mut rest = body;
    let mut offset = 0;
    while let Some(position) = rest.find("T") {
        let start = offset + position;
        if start >= 10 && body.is_char_boundary(start - 10) {
            // Include a trailing offset when the body carries one.
            let end = (start + 12).min(body.len());
            let end = (start..=end)
                .rev()
                .find(|edge| body.is_char_boundary(*edge))
                .unwrap_or(start);
            let candidate = &body[start - 10..end];
            if let Some(unix) = unix_from_iso(candidate) {
                found.push(unix);
            }
        }
        offset = start + 1;
        rest = &body[offset..];
    }
    found.into_iter()
}

/// `YYYYMMDDHH` or `YYYYMMDDHHMM` to Unix seconds (Hong Kong time).
fn compact_stamp(run: &str) -> Option<i64> {
    if !matches!(run.len(), 10 | 12 | 14) {
        return None;
    }
    let year: i64 = run.get(0..4)?.parse().ok()?;
    let month: u32 = run.get(4..6)?.parse().ok()?;
    let day: u32 = run.get(6..8)?.parse().ok()?;
    let hour: i64 = run.get(8..10)?.parse().ok()?;
    let minute: i64 = run.get(10..12).map_or(Some(0), |raw| raw.parse().ok())?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 {
        return None;
    }
    Some(crate::days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 - 8 * 3_600)
}

/// ISO-8601 with an offset to Unix seconds.
fn unix_from_iso(iso: &str) -> Option<i64> {
    let iso = iso.trim();
    let (date, rest) = iso.split_once('T')?;
    let days = crate::days_from_iso(date)?;
    let hour: i64 = rest.get(0..2)?.parse().ok()?;
    let minute: i64 = rest.get(3..5).map_or(Some(0), |raw| raw.parse().ok())?;
    // These are Hong Kong feeds, so a missing offset means +08:00.
    let offset = if let Some(position) = rest.find(['+', '-']) {
        let tail = &rest[position..];
        let sign: i64 = if tail.starts_with('-') { -1 } else { 1 };
        let digits = tail.get(1..).unwrap_or_default();
        let (hours, minutes) = if let Some((hours, minutes)) = digits.split_once(':') {
            (hours, minutes)
        } else if digits.len() >= 4 {
            (&digits[0..2], &digits[2..4])
        } else {
            (digits, "0")
        };
        let hours: i64 = hours.parse().map_err(|_| ()).ok()?;
        let minutes: i64 = minutes.parse().unwrap_or(0);
        sign * (hours * 3_600 + minutes * 60)
    } else if rest.contains('Z') {
        0
    } else {
        8 * 3_600
    };
    Some(days * 86_400 + hour * 3_600 + minute * 60 - offset)
}

/// Remove HTML tags and collapse runs of whitespace.
///
/// HKO warnings and the app API health advice both arrive as small HTML
/// fragments (`<p>`, `<b>`, `<br>`); the text is what we want.
#[must_use]
pub fn strip_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut inside_tag = false;
    for character in input.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => out.push(character),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn value_text(value: &Value, key: &str) -> Option<String> {
    match value.get(key) {
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(Value::Number(number)) => Some(number.to_string()),
        _ => None,
    }
}

fn value_i64(value: &Value, key: &str) -> Option<i64> {
    match value.get(key) {
        Some(Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_f64().map(|float| float.round() as i64)),
        Some(Value::String(text)) => text
            .trim()
            .parse::<i64>()
            .ok()
            .or_else(|| parse_measurement(text).map(|float| float.round() as i64)),
        _ => None,
    }
}

fn value_f64(value: &Value, key: &str) -> Option<f64> {
    match value.get(key) {
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => parse_measurement(text),
        _ => None,
    }
}

/// Parse the EPD app API forecast payload into its two unjoined lists.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the document is not an object or carries no
/// forecast period at all.
pub fn parse_forecast_entries(input: &str) -> Result<ParsedForecast> {
    let root: Value = serde_json::from_str(strip_bom(input))?;
    if !root.is_object() {
        return Err(Error::parse(
            "EPD forecast JSON: expected a top-level object",
        ));
    }
    let general = forecast_entries(&root, "general_stations");
    let roadside = forecast_entries(&root, "roadside_stations");
    if general.is_empty() && roadside.is_empty() {
        return Err(Error::parse("EPD forecast JSON: no forecast periods found"));
    }
    Ok(ParsedForecast { general, roadside })
}

fn forecast_entries(root: &Value, key: &str) -> Vec<ForecastEntry> {
    let Some(items) = root.get(key).and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            Some(ForecastEntry {
                period: value_text(item, "forecast_period_en")?,
                min: value_i64(item, "min")?,
                max: value_i64(item, "max")?,
            })
        })
        .collect()
}

/// Join the general and roadside forecast lists by position and period name.
///
/// General-station order leads; a roadside entry is matched by index when the
/// names agree there, otherwise by name. Periods that exist on only one side
/// are still emitted with `None` for the missing side, and any leftover
/// roadside-only periods are appended in payload order.
#[must_use]
pub fn join_forecast(general: &[ForecastEntry], roadside: &[ForecastEntry]) -> Vec<ForecastPeriod> {
    let mut used = vec![false; roadside.len()];
    let mut periods = Vec::with_capacity(general.len() + roadside.len());
    for (index, entry) in general.iter().enumerate() {
        let mut matched = None;
        if roadside
            .get(index)
            .is_some_and(|candidate| candidate.period == entry.period)
        {
            matched = Some(index);
        }
        if matched.is_none() {
            matched = roadside
                .iter()
                .enumerate()
                .position(|(at, candidate)| !used[at] && candidate.period == entry.period);
        }
        let roadside_range = matched.map(|at| {
            used[at] = true;
            roadside[at].range()
        });
        periods.push(ForecastPeriod {
            period: entry.period.clone(),
            general: Some(entry.range()),
            roadside: roadside_range,
        });
    }
    for (index, entry) in roadside.iter().enumerate() {
        if !used[index] {
            periods.push(ForecastPeriod {
                period: entry.period.clone(),
                general: None,
                roadside: Some(entry.range()),
            });
        }
    }
    periods
}

/// Parse and join the EPD app API forecast payload.
///
/// # Errors
///
/// Propagates [`parse_forecast_entries`] failures.
pub fn parse_forecast_json(input: &str) -> Result<Vec<ForecastPeriod>> {
    let parsed = parse_forecast_entries(input)?;
    Ok(join_forecast(&parsed.general, &parsed.roadside))
}

/// Parse the app API `/monitors` array.
///
/// Monitor ids are mapped to canonical English names through
/// [`crate::model::EPD_APP_STATIONS`]; an id outside that table keeps the
/// published (Traditional Chinese) name.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the payload is not an array or yields no
/// station.
pub fn parse_monitors_json(input: &str) -> Result<Vec<MonitorStation>> {
    let root: Value = serde_json::from_str(strip_bom(input))?;
    let array = root
        .as_array()
        .ok_or_else(|| Error::parse("EPD monitors JSON: expected a top-level array"))?;
    let mut stations = Vec::new();
    for item in array {
        let Some(id) = value_i64(item, "id") else {
            continue;
        };
        let station = app_station_name(id)
            .map(str::to_string)
            .or_else(|| value_text(item, "name"))
            .unwrap_or_else(|| format!("station {id}"));
        let category = value_i64(item, "category").unwrap_or(0);
        stations.push(MonitorStation {
            id,
            station,
            category: u8::try_from(category).unwrap_or(0),
            latitude: value_f64(item, "latitude"),
            longitude: value_f64(item, "longitude"),
            latest: latest_reading(item, "updated_at"),
        });
    }
    if stations.is_empty() {
        return Err(Error::parse("EPD monitors JSON: no station entries found"));
    }
    Ok(stations)
}

/// Parse one app API `/monitors/{id}` object, merging its seven histories into
/// one newest-first series.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the document is not an object, has no `id` or
/// carries no history at all.
pub fn parse_monitor_detail_json(input: &str) -> Result<MonitorDetail> {
    let root: Value = serde_json::from_str(strip_bom(input))?;
    if !root.is_object() {
        return Err(Error::parse(
            "EPD monitor JSON: expected a top-level object",
        ));
    }
    let id = value_i64(&root, "id")
        .ok_or_else(|| Error::parse("EPD monitor JSON: missing monitor id"))?;
    let series = history_series(&root);
    if series.is_empty() {
        return Err(Error::parse(format!(
            "EPD monitor JSON: no history found for station {id}"
        )));
    }
    Ok(MonitorDetail {
        id,
        latest: latest_reading(&root, "updated_at"),
        series,
    })
}

fn latest_reading(value: &Value, time_key: &str) -> Option<HistoryReading> {
    let time = value_text(value, time_key).map(|raw| normalize_datetime(&raw))?;
    Some(HistoryReading {
        time,
        aqhi: value_i64(value, "aqhi"),
        no2: value_f64(value, "no2"),
        o3: value_f64(value, "o3"),
        so2: value_f64(value, "so2"),
        co: value_f64(value, "co"),
        pm10: value_f64(value, "pm10"),
        pm25: value_f64(value, "pm25"),
    })
}

fn history_series(root: &Value) -> Vec<HistoryReading> {
    let aqhi = history_values(root, "aqhi_histories");
    let no2 = history_values(root, "no2_histories");
    let o3 = history_values(root, "o3_histories");
    let so2 = history_values(root, "so2_histories");
    let co = history_values(root, "co_histories");
    let pm10 = history_values(root, "pm10_histories");
    let pm25 = history_values(root, "pm25_histories");
    let mut times: BTreeSet<&String> = BTreeSet::new();
    for series in [&aqhi, &no2, &o3, &so2, &co, &pm10, &pm25] {
        times.extend(series.keys());
    }
    times
        .into_iter()
        .rev()
        .map(|time| HistoryReading {
            time: normalize_datetime(time),
            aqhi: aqhi
                .get(time)
                .copied()
                .flatten()
                .map(|value| value.round() as i64),
            no2: no2.get(time).copied().flatten(),
            o3: o3.get(time).copied().flatten(),
            so2: so2.get(time).copied().flatten(),
            co: co.get(time).copied().flatten(),
            pm10: pm10.get(time).copied().flatten(),
            pm25: pm25.get(time).copied().flatten(),
        })
        .collect()
}

fn history_values(root: &Value, key: &str) -> BTreeMap<String, Option<f64>> {
    let mut values = BTreeMap::new();
    let Some(items) = root.get(key).and_then(Value::as_array) else {
        return values;
    };
    for item in items {
        if let Some(created_at) = value_text(item, "created_at") {
            values.insert(created_at, value_f64(item, "value"));
        }
    }
    values
}

/// Parse the PRAISE-HK `pollutant_index` payload into hourly readings.
///
/// Each series is aligned by position with `isots`; a null or absent value
/// stays `None`. Pollutant values and the added health risk are rounded to one
/// decimal place. The upstream order is kept: the first entry is the current
/// hour and later entries extend into the forecast.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the document is not an object, reports a
/// non-zero `status`, has no `isots` array or yields no reading.
pub fn parse_praise_json(input: &str) -> Result<Vec<StreetReading>> {
    let root: Value = serde_json::from_str(strip_bom(input))?;
    if !root.is_object() {
        return Err(Error::parse("PRAISE-HK JSON: expected a top-level object"));
    }
    if let Some(status) = root.get("status").and_then(Value::as_i64) {
        if status != 0 {
            return Err(Error::parse(format!(
                "PRAISE-HK JSON: upstream status {status}"
            )));
        }
    }
    let Some(times) = root.get("isots").and_then(Value::as_array) else {
        return Err(Error::parse("PRAISE-HK JSON: missing isots array"));
    };
    let aqhi = json_array(&root, "AQHIBN");
    let added_health_risk = json_array(&root, "AQHIER");
    let no2 = json_array(&root, "NO2");
    let o3 = json_array(&root, "O3");
    let pm10 = json_array(&root, "PM10");
    let pm25 = json_array(&root, "PM2.5");
    let so2 = json_array(&root, "SO2");
    let mut readings = Vec::with_capacity(times.len());
    for (index, time) in times.iter().enumerate() {
        let Some(time) = time.as_str().map(str::trim).filter(|time| !time.is_empty()) else {
            continue;
        };
        readings.push(StreetReading {
            time: normalize_datetime(time),
            aqhi: array_f64(&aqhi, index).map(|value| value.round() as i64),
            added_health_risk: array_f64(&added_health_risk, index).map(round_one_decimal),
            no2: array_f64(&no2, index).map(round_one_decimal),
            o3: array_f64(&o3, index).map(round_one_decimal),
            pm10: array_f64(&pm10, index).map(round_one_decimal),
            pm25: array_f64(&pm25, index).map(round_one_decimal),
            so2: array_f64(&so2, index).map(round_one_decimal),
        });
    }
    if readings.is_empty() {
        return Err(Error::parse("PRAISE-HK JSON: no readings found"));
    }
    Ok(readings)
}

fn json_array(root: &Value, key: &str) -> Vec<Value> {
    root.get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn array_f64(values: &[Value], index: usize) -> Option<f64> {
    match values.get(index)? {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => parse_measurement(text),
        _ => None,
    }
}

fn round_one_decimal(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// Parse the app API `/health_advices` payload into `(band_index, row)` pairs.
///
/// The upstream rows are Traditional Chinese with `<p>` markup; tags are
/// stripped and whitespace collapsed. Entries whose category is outside 0-4 are
/// skipped, and a document with no usable row is a parse error.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the body is not a JSON array or carries no
/// advice row.
pub fn parse_health_advices_json(input: &str) -> Result<Vec<(usize, AdviceRow)>> {
    let root: Value = serde_json::from_str(strip_bom(input))?;
    let array = root
        .as_array()
        .ok_or_else(|| Error::parse("health advice JSON: expected a top-level array"))?;
    let mut rows = Vec::new();
    for item in array {
        let Some(category) = value_i64(item, "health_risk_category") else {
            continue;
        };
        let Ok(index) = usize::try_from(category) else {
            continue;
        };
        if index > 4 {
            continue;
        }
        let row = AdviceRow {
            heart_and_respiratory: advice_field(item, "people_with_existing_heart"),
            children_and_elderly: advice_field(item, "children_and_the_elderly"),
            outdoor_workers: advice_field(item, "outdoor_workers"),
            general_public: advice_field(item, "general_public"),
        };
        if row.general_public.is_empty() && row.heart_and_respiratory.is_empty() {
            continue;
        }
        rows.push((index, row));
    }
    if rows.is_empty() {
        return Err(Error::parse("health advice JSON: no advice rows found"));
    }
    Ok(rows)
}

fn advice_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(strip_tags)
        .unwrap_or_default()
}

/// Parse the PRAISE-HK reverse-geocoding payload and return the first name.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the body is not an object, reports
/// `success: false`, or carries no non-empty name.
pub fn parse_reverse_geocoding_json(input: &str) -> Result<String> {
    let root: Value = serde_json::from_str(strip_bom(input))?;
    if !root.is_object() {
        return Err(Error::parse(
            "reverse geocoding JSON: expected a top-level object",
        ));
    }
    if root.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(Error::parse(
            "reverse geocoding JSON: upstream reported failure",
        ));
    }
    let name = root
        .get("result")
        .and_then(Value::as_array)
        .and_then(|results| results.first())
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| Error::parse("reverse geocoding JSON: no place name returned"))?;
    Ok(name.to_string())
}

/// Parsed HKO nine-day forecast: the general situation plus the days.
pub struct ParsedForecastDays {
    /// General situation paragraph.
    pub situation: Option<String>,
    /// Forecast days, soonest first.
    pub days: Vec<ForecastDay>,
}

/// Parse the HKO `fnd` payload.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the body is not an object or carries no
/// `weatherForecast` day.
pub fn parse_hko_forecast_json(input: &str) -> Result<ParsedForecastDays> {
    let root: Value = serde_json::from_str(strip_bom(input))?;
    if !root.is_object() {
        return Err(Error::parse(
            "HKO forecast JSON: expected a top-level object",
        ));
    }
    let situation = value_text(&root, "generalSituation");
    let array = root
        .get("weatherForecast")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::parse("HKO forecast JSON: missing weatherForecast"))?;
    let mut days = Vec::new();
    for item in array {
        let Some(date) = value_text(item, "forecastDate") else {
            continue;
        };
        days.push(ForecastDay {
            date,
            week: value_text(item, "week").unwrap_or_default(),
            weather: value_text(item, "forecastWeather"),
            max_temp: nested_i64(item, "forecastMaxtemp", "value"),
            min_temp: nested_i64(item, "forecastMintemp", "value"),
            max_humidity: nested_i64(item, "forecastMaxrh", "value")
                .or_else(|| nested_i64(item, "forecastMaxRH", "value")),
            min_humidity: nested_i64(item, "forecastMinrh", "value")
                .or_else(|| nested_i64(item, "forecastMinRH", "value")),
            psr: value_text(item, "PSR")
                .or_else(|| value_text(item, "psr"))
                .map(|value| match value.as_str() {
                    "low" => "Low".to_string(),
                    "medium" => "Medium".to_string(),
                    "high" => "High".to_string(),
                    other => other.to_string(),
                }),
        });
    }
    if days.is_empty() {
        return Err(Error::parse("HKO forecast JSON: no forecast days found"));
    }
    Ok(ParsedForecastDays { situation, days })
}

fn nested_i64(value: &Value, key: &str, inner: &str) -> Option<i64> {
    let nested = value.get(key)?;
    value_i64(nested, inner).or_else(|| value_i64(value, key))
}

/// Parsed PRAISE-HK spatial tile response.
pub struct ParsedTile {
    /// Tile URL, as published.
    pub url: String,
    /// Timestamp label, `YYYYMMDDHH`.
    pub ts: Option<String>,
    /// Coverage bounds: `(lat0, lng0, lat1, lng1)`.
    pub coverage: (f64, f64, f64, f64),
}

/// Parse the PRAISE-HK `pollutant_spatial_tiles` payload.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the body carries no tile URL or reports a
/// non-zero `status`.
pub fn parse_spatial_tile_json(input: &str) -> Result<ParsedTile> {
    let root: Value = serde_json::from_str(strip_bom(input))?;
    if !root.is_object() {
        return Err(Error::parse(
            "spatial tiles JSON: expected a top-level object",
        ));
    }
    if let Some(status) = value_i64(&root, "status") {
        if status != 0 {
            let message = value_text(&root, "msg").unwrap_or_default();
            return Err(Error::parse(format!(
                "spatial tiles JSON: upstream status {status} {message}"
            )));
        }
    }
    let tiles = root
        .get("tiles")
        .ok_or_else(|| Error::parse("spatial tiles JSON: missing tiles"))?;
    let url = value_text(tiles, "url")
        .ok_or_else(|| Error::parse("spatial tiles JSON: missing tile url"))?;
    let coverage = (
        value_f64(tiles, "lat0").unwrap_or(f64::NAN),
        value_f64(tiles, "lng0").unwrap_or(f64::NAN),
        value_f64(tiles, "lat1").unwrap_or(f64::NAN),
        value_f64(tiles, "lng1").unwrap_or(f64::NAN),
    );
    Ok(ParsedTile {
        url,
        ts: value_text(&root, "ts"),
        coverage,
    })
}

/// One "about" section from the EPD app API.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AboutSection {
    /// Section title.
    pub title: String,
    /// Section body with markup stripped.
    pub content: String,
    /// Link published with the section, when any.
    pub url: Option<String>,
}

/// Parse the app API `/abouts` payload.
///
/// # Errors
///
/// Returns [`Error::Parse`] when the body is not an array or carries no titled
/// section.
pub fn parse_abouts_json(input: &str) -> Result<Vec<AboutSection>> {
    let root: Value = serde_json::from_str(strip_bom(input))?;
    let array = root
        .as_array()
        .ok_or_else(|| Error::parse("abouts JSON: expected a top-level array"))?;
    let mut sections = Vec::new();
    for item in array {
        let Some(title) = value_text(item, "title") else {
            continue;
        };
        let content = item
            .get("content")
            .and_then(Value::as_str)
            .map(strip_tags)
            .unwrap_or_default();
        sections.push(AboutSection {
            title,
            content,
            url: value_text(item, "url"),
        });
    }
    if sections.is_empty() {
        return Err(Error::parse("abouts JSON: no sections found"));
    }
    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::latest_timestamp;

    #[test]
    fn reads_tagged_dates() {
        let body = "<rss><channel><lastBuildDate>Sun, 22 Nov 2026 01:30:00 +0800</lastBuildDate></channel></rss>";
        assert_eq!(latest_timestamp(body), Some(1_795_282_200));
    }

    #[test]
    fn reads_compact_and_iso_stamps() {
        let unix = 1_795_280_400;
        assert_eq!(
            latest_timestamp("Date time,Value\n2026112201,0.09"),
            Some(unix)
        );
        assert_eq!(
            latest_timestamp(r#"{"time":"2026-11-22T01:00:00+08:00"}"#),
            Some(unix)
        );
        assert_eq!(latest_timestamp("no dates here"), None);
    }
}
