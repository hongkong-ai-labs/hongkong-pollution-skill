//! `hkpollution` command-line interface.

use std::process::exit;

use hongkong_pollution_skill::model::{
    AqhiReport, Averages, ForecastRange, ForecastReport, HistoryReading, HistoryReport,
    PollutantsReport, SummaryReport, WeatherReport, mean, resolve_station,
};
use hongkong_pollution_skill::{Error, Result, VERSION, aqhi, epd_app, mcp, pollutants, weather};

const USAGE: &str = "\
hkpollution - Hong Kong air pollution data (EPD AQHI + HKO weather)

USAGE:
    hkpollution aqhi       [--station <name>] [--json]
    hkpollution pollutants [--station <name>] [--hours <1-24>] [--json]
    hkpollution forecast   [--json]
    hkpollution history    [--station <name>] [--hours <1-23>] [--json]
    hkpollution summary    [--json] [--with-weather]
    hkpollution weather    [--json]
    hkpollution mcp
    hkpollution --help | --version

OPTIONS:
    --station <name>   case-insensitive station substring; ambiguous names return all matches
    --hours <N>        newest hourly readings kept per station: 1-24 for `pollutants`,
                       1-23 for `history` (default 1)
    --json             print one JSON object to stdout and nothing else
    --with-weather     add HKO temperature, humidity and warnings to the summary

EXIT CODES:
    0 success, 1 network/upstream/io failure, 2 usage error";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match run(&args) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("hkpollution: {error}");
            if error.is_usage() {
                eprintln!();
                eprintln!("{USAGE}");
                2
            } else {
                1
            }
        }
    };
    exit(code);
}

fn run(args: &[String]) -> Result<()> {
    let Some(command) = args.first() else {
        return Err(Error::usage("missing command"));
    };
    match command.as_str() {
        "--help" | "-h" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        "--version" | "-V" | "version" => {
            println!("hkpollution {VERSION}");
            Ok(())
        }
        "mcp" => {
            if args.len() > 1 {
                return Err(Error::usage("`mcp` does not take arguments"));
            }
            mcp::serve()
        }
        "aqhi" => command_aqhi(&args[1..]),
        "pollutants" => command_pollutants(&args[1..]),
        "forecast" => command_forecast(&args[1..]),
        "history" => command_history(&args[1..]),
        "summary" => command_summary(&args[1..]),
        "weather" => command_weather(&args[1..]),
        other => Err(Error::usage(format!("unknown command: {other}"))),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    Aqhi,
    Pollutants,
    Forecast,
    History,
    Summary,
    Weather,
}

impl Command {
    fn name(self) -> &'static str {
        match self {
            Self::Aqhi => "aqhi",
            Self::Pollutants => "pollutants",
            Self::Forecast => "forecast",
            Self::History => "history",
            Self::Summary => "summary",
            Self::Weather => "weather",
        }
    }

    fn allows_station(self) -> bool {
        matches!(self, Self::Aqhi | Self::Pollutants | Self::History)
    }

    fn max_hours(self) -> Option<u32> {
        match self {
            Self::Pollutants => Some(pollutants::MAX_HOURS),
            Self::History => Some(epd_app::MAX_HOURS),
            _ => None,
        }
    }

    fn allows_weather(self) -> bool {
        matches!(self, Self::Summary)
    }
}

#[derive(Default)]
struct Options {
    json: bool,
    with_weather: bool,
    help: bool,
    station: Option<String>,
    hours: Option<u32>,
}

fn parse_options(args: &[String], command: Command) -> Result<Options> {
    let mut options = Options::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--json" {
            options.json = true;
            index += 1;
        } else if arg == "--help" || arg == "-h" {
            options.help = true;
            index += 1;
        } else if arg == "--with-weather" {
            if !command.allows_weather() {
                return Err(Error::usage(format!(
                    "--with-weather is only valid for `summary`, not `{}`",
                    command.name()
                )));
            }
            options.with_weather = true;
            index += 1;
        } else if arg == "--station" || arg.starts_with("--station=") {
            if !command.allows_station() {
                return Err(Error::usage(format!(
                    "--station is only valid for `aqhi`, `pollutants` and `history`, not `{}`",
                    command.name()
                )));
            }
            let value = match arg.strip_prefix("--station=") {
                Some(inline) => {
                    index += 1;
                    inline.to_string()
                }
                None => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| Error::usage("--station requires a station name"))?;
                    index += 2;
                    value.clone()
                }
            };
            if value.trim().is_empty() {
                return Err(Error::usage("--station requires a non-empty station name"));
            }
            options.station = Some(value);
        } else if arg == "--hours" || arg.starts_with("--hours=") {
            let Some(max_hours) = command.max_hours() else {
                return Err(Error::usage(format!(
                    "--hours is only valid for `pollutants` and `history`, not `{}`",
                    command.name()
                )));
            };
            let value = match arg.strip_prefix("--hours=") {
                Some(inline) => {
                    index += 1;
                    inline.to_string()
                }
                None => {
                    let value = args.get(index + 1).ok_or_else(|| {
                        Error::usage(format!(
                            "--hours requires a number between 1 and {max_hours}"
                        ))
                    })?;
                    index += 2;
                    value.clone()
                }
            };
            let hours: u32 = value.trim().parse().map_err(|_| {
                Error::usage(format!(
                    "--hours must be a number between 1 and {max_hours}, got {value:?}"
                ))
            })?;
            if hours == 0 || hours > max_hours {
                return Err(Error::usage(format!(
                    "--hours must be a number between 1 and {max_hours}"
                )));
            }
            options.hours = Some(hours);
        } else {
            return Err(Error::usage(format!(
                "unknown argument for `{}`: {arg}",
                command.name()
            )));
        }
    }
    Ok(options)
}

fn command_aqhi(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Aqhi)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    let mut report = aqhi::fetch()?;
    if let Some(query) = &options.station {
        let names = resolve_station(query)?;
        aqhi::retain_stations(&mut report.stations, &names);
    }
    if options.json {
        print_json(&report)
    } else {
        print_aqhi(&report);
        Ok(())
    }
}

fn command_pollutants(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Pollutants)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    let mut report = pollutants::fetch(options.hours.unwrap_or(1))?;
    if let Some(query) = &options.station {
        let names = resolve_station(query)?;
        pollutants::retain_stations(&mut report, &names);
    }
    if options.json {
        print_json(&report)
    } else {
        print_pollutants(&report);
        Ok(())
    }
}

fn command_forecast(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Forecast)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    let report = epd_app::fetch_forecast()?;
    if options.json {
        print_json(&report)
    } else {
        print_forecast(&report);
        Ok(())
    }
}

fn command_history(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::History)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    let report = epd_app::fetch_history(options.station.as_deref(), options.hours.unwrap_or(1))?;
    if options.json {
        print_json(&report)
    } else {
        print_history(&report);
        Ok(())
    }
}

fn command_summary(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Summary)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    let report = aqhi::summary(options.with_weather)?;
    if options.json {
        print_json(&report)
    } else {
        print_summary(&report);
        Ok(())
    }
}

fn command_weather(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Weather)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    let report = weather::fetch()?;
    if options.json {
        print_json(&report)
    } else {
        print_weather(&report);
        Ok(())
    }
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    let encoded = serde_json::to_string(value)?;
    println!("{encoded}");
    Ok(())
}

fn print_aqhi(report: &AqhiReport) {
    let rows: Vec<Vec<String>> = report
        .stations
        .iter()
        .map(|reading| {
            vec![
                reading.station.clone(),
                format_i64(reading.aqhi),
                reading
                    .health_risk
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
                reading
                    .publish_date
                    .as_deref()
                    .map_or_else(|| "-".to_string(), pretty_time),
            ]
        })
        .collect();
    print_table(&["Station", "AQHI", "Risk", "Published"], &rows);
    println!();
    println!(
        "source: EPD Air Quality Health Index (fetched {})",
        report.fetched_at
    );
}

fn print_pollutants(report: &PollutantsReport) {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for station in &report.stations {
        for reading in &station.series {
            rows.push(pollutant_row(
                &station.station,
                reading.time.as_str(),
                reading,
            ));
        }
        if station.series.len() > 1 {
            let averages = &station.averages;
            rows.push(vec![
                station.station.clone(),
                format!("avg {}", station.series.len()),
                format_f64(averages.no2),
                format_f64(averages.o3),
                format_f64(averages.so2),
                format_f64(averages.co),
                format_f64(averages.pm10),
                format_f64(averages.pm25),
            ]);
        }
    }
    print_table(
        &["Station", "Time", "NO2", "O3", "SO2", "CO", "PM10", "PM2.5"],
        &rows,
    );
    println!();
    let build = report.last_build_date.as_deref().unwrap_or("unknown");
    println!("source: EPD 24-hour pollutant concentrations in µg/m³ (last build: {build})");
}

fn pollutant_row(
    station: &str,
    time: &str,
    reading: &hongkong_pollution_skill::model::PollutantReading,
) -> Vec<String> {
    vec![
        station.to_string(),
        pretty_time(time),
        format_f64(reading.no2),
        format_f64(reading.o3),
        format_f64(reading.so2),
        format_f64(reading.co),
        format_f64(reading.pm10),
        format_f64(reading.pm25),
    ]
}

fn print_forecast(report: &ForecastReport) {
    let rows: Vec<Vec<String>> = report
        .periods
        .iter()
        .map(|period| {
            vec![
                period.period.clone(),
                format_range(period.general),
                format_range(period.roadside),
            ]
        })
        .collect();
    print_table(&["Period", "General", "Roadside"], &rows);
    println!();
    println!(
        "source: EPD app API (fetched {}) — EPD data © Government of the HKSAR",
        report.fetched_at
    );
}

fn format_range(range: Option<ForecastRange>) -> String {
    range.map_or_else(
        || "-".to_string(),
        |range| format!("{}-{}", range.min, range.max),
    )
}

fn print_history(report: &HistoryReport) {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for station in &report.stations {
        let shown = &station.series[..(report.hours as usize).min(station.series.len())];
        for reading in shown {
            rows.push(history_row(&station.station, reading));
        }
        if shown.len() > 1 {
            let averages = Averages::from_history(shown);
            let aqhi_values: Vec<f64> = shown
                .iter()
                .filter_map(|reading| reading.aqhi)
                .map(|aqhi| aqhi as f64)
                .collect();
            rows.push(vec![
                station.station.clone(),
                format!("avg {}", shown.len()),
                format_f64(mean(&aqhi_values)),
                format_f64(averages.no2),
                format_f64(averages.o3),
                format_f64(averages.so2),
                format_f64(averages.co),
                format_f64(averages.pm10),
                format_f64(averages.pm25),
            ]);
        }
    }
    print_table(
        &[
            "Station", "Time", "AQHI", "NO2", "O3", "SO2", "CO", "PM10", "PM2.5",
        ],
        &rows,
    );
    println!();
    println!(
        "source: EPD app API (fetched {}) — EPD data © Government of the HKSAR",
        report.fetched_at
    );
}

fn history_row(station: &str, reading: &HistoryReading) -> Vec<String> {
    vec![
        station.to_string(),
        pretty_time(&reading.time),
        format_i64(reading.aqhi),
        format_f64(reading.no2),
        format_f64(reading.o3),
        format_f64(reading.so2),
        format_f64(reading.co),
        format_f64(reading.pm10),
        format_f64(reading.pm25),
    ]
}

fn print_summary(report: &SummaryReport) {
    let rows: Vec<Vec<String>> = report
        .stations
        .iter()
        .map(|reading| {
            vec![
                reading.station.clone(),
                format_i64(reading.aqhi),
                reading
                    .health_risk
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            ]
        })
        .collect();
    print_table(&["Station", "AQHI", "Risk"], &rows);
    println!();
    match &report.worst {
        Some(worst) => println!(
            "network: worst {} ({}) / mean {}",
            format_i64(worst.aqhi),
            worst.station,
            format_f64(report.mean_aqhi)
        ),
        None => println!("network: no AQHI readings available"),
    }
    if !report.risk_counts.is_empty() {
        let counts: Vec<String> = report
            .risk_counts
            .iter()
            .map(|(risk, count)| format!("{risk} {count}"))
            .collect();
        println!("risk levels: {}", counts.join(", "));
    }
    match &report.worst_pm25 {
        Some(extreme) => println!(
            "worst PM2.5: {} {} µg/m³ ({})",
            extreme.station,
            format_f64(Some(extreme.value)),
            pretty_time(&extreme.time)
        ),
        None => println!("worst PM2.5: no latest reading"),
    }
    match &report.worst_no2 {
        Some(extreme) => println!(
            "worst NO2: {} {} µg/m³ ({})",
            extreme.station,
            format_f64(Some(extreme.value)),
            pretty_time(&extreme.time)
        ),
        None => println!("worst NO2: no latest reading"),
    }
    if let Some(weather) = &report.weather {
        let temperature = weather
            .temperature
            .iter()
            .find(|reading| reading.place.eq_ignore_ascii_case("Hong Kong Observatory"))
            .or_else(|| weather.temperature.first());
        let humidity = weather
            .humidity
            .iter()
            .find(|reading| reading.place.eq_ignore_ascii_case("Hong Kong Observatory"))
            .or_else(|| weather.humidity.first());
        println!(
            "weather: {} temperature {} °C, humidity {} %{}",
            weather.update_time.as_deref().unwrap_or("unknown time"),
            format_f64(temperature.and_then(|reading| reading.value)),
            format_f64(humidity.and_then(|reading| reading.value)),
            if weather.warnings.is_empty() {
                ", no warnings".to_string()
            } else {
                format!(", warnings: {}", weather.warnings.join("; "))
            }
        );
    }
    println!();
    println!("source: {}", report.source);
}

fn print_weather(report: &WeatherReport) {
    let mut rows: Vec<Vec<String>> = Vec::new();
    for reading in &report.temperature {
        rows.push(vec![
            "Temperature".to_string(),
            reading.place.clone(),
            format_f64(reading.value),
            reading.unit.clone().unwrap_or_else(|| "C".to_string()),
        ]);
    }
    for reading in &report.humidity {
        rows.push(vec![
            "Humidity".to_string(),
            reading.place.clone(),
            format_f64(reading.value),
            reading
                .unit
                .clone()
                .unwrap_or_else(|| "percent".to_string()),
        ]);
    }
    print_table(&["Reading", "Location", "Value", "Unit"], &rows);
    println!();
    println!(
        "update time: {}",
        report.update_time.as_deref().unwrap_or("unknown")
    );
    if report.warnings.is_empty() {
        println!("warnings: none");
    } else {
        for warning in &report.warnings {
            println!("warning: {warning}");
        }
    }
    println!();
    println!("source: HKO current readings");
}

fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    let mut widths: Vec<usize> = headers
        .iter()
        .map(|header| header.chars().count())
        .collect();
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            if index < widths.len() {
                widths[index] = widths[index].max(cell.chars().count());
            }
        }
    }
    let header_cells: Vec<String> = headers.iter().map(|header| (*header).to_string()).collect();
    println!("{}", render_row(&header_cells, &widths));
    let rule: Vec<String> = widths.iter().map(|width| "-".repeat(*width)).collect();
    println!("{}", render_row(&rule, &widths));
    for row in rows {
        println!("{}", render_row(row, &widths));
    }
}

fn render_row(cells: &[String], widths: &[usize]) -> String {
    let mut line = String::new();
    for (index, cell) in cells.iter().enumerate() {
        if index > 0 {
            line.push_str("  ");
        }
        let width = widths.get(index).copied().unwrap_or(0);
        line.push_str(&format!("{cell:<width$}"));
    }
    line.trim_end().to_string()
}

fn format_i64(value: Option<i64>) -> String {
    value.map_or_else(|| "-".to_string(), |number| number.to_string())
}

fn format_f64(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_string(), |number| format!("{number:.1}"))
}

fn pretty_time(raw: &str) -> String {
    let text = raw.trim();
    if text.is_empty() {
        return "-".to_string();
    }
    let replaced = text.replacen('T', " ", 1);
    replaced.chars().take(16).collect()
}
