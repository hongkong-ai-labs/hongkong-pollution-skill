//! Pure parsers for every upstream payload. Nothing here touches the network.
//!
//! The XML side uses a small hand-written tag scanner: enough to walk the two
//! EPD documents, deliberately tolerant about attributes, CDATA and stray
//! whitespace.

use serde_json::Value;

use crate::error::{Error, Result};
use crate::model::{
    AQHI_STATIONS, AqhiReading, HkoReading, ParsedPollutants, PollutantReading, PollutantRecord,
    WeatherReport,
};

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

fn strip_tags(input: &str) -> String {
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
