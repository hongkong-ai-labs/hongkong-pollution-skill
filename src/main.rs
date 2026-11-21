//! `hkpollution` command-line interface.

use std::process::exit;

use hongkong_pollution_skill::model::{
    AqhiReport, Averages, ForecastRange, ForecastReport, HistoryReading, HistoryReport,
    PollutantsReport, StreetReport, SummaryReport, WeatherReport, at_or_above, mean,
    nearest_station, resolve_station, rollup,
};
use hongkong_pollution_skill::{
    Error, Result, VERSION, advice, advice::Lang, aqhi, aqhi24, beach, brief, digest, doctor,
    epd_app, http, mcp, pollutants, praise, radiation, recycling, redtide, stats, uv, watch, water,
    weather,
};

mod update;

/// Highest AQHI value accepted by `--fail-above`.
const MAX_AQHI: i64 = 11;

/// Hours compared by the `summary` trend.
const TREND_HOURS: u32 = 4;

const USAGE: &str = "\
hkpollution - Hong Kong air pollution data (EPD AQHI + HKO weather)

USAGE:
    hkpollution aqhi       [--station <name> | --near <lat>,<lng>] [--fail-above <N>] [--json]
    hkpollution pollutants [--station <name>] [--hours <1-24>] [--json]
    hkpollution forecast   [--json]
    hkpollution history    [--station <name> | --near <lat>,<lng>] [--hours <1-23>] [--json]
    hkpollution street     --lat <22.x> --lng <114.x> [--hours <1-48>] [--json]
    hkpollution advice     [--risk <band>] [--lang <en|tc|both>] [--json]
    hkpollution brief      --lat <22.x> --lng <114.x> [--fail-above <N>] [--json]
    hkpollution summary    [--station <name> | --near <lat>,<lng>] [--fail-above <N>]
                           [--json] [--with-weather]
    hkpollution weather    [--forecast] [--json]
    hkpollution beach      [--name <text>] [--district <text>] [--grade <band>]
                           [--near <lat>,<lng>] [--history <0-10>] [--json]
    hkpollution river      [--zone <text>] [--station <name>] [--months <1-60>] [--json]
    hkpollution marine     [--zone <text>] [--station <name>] [--months <1-60>] [--json]
    hkpollution red-tide   [--days <1-3650>] [--active] [--near <lat>,<lng>] [--json]
    hkpollution radiation  [--station <name>] [--json]
    hkpollution uv         [--json]
    hkpollution stats      [--days <1-365>] [--station <name>] [--threshold <1-11>] [--json]
    hkpollution compare    [--json]
    hkpollution digest     [--json]
    hkpollution recycling  --lat <22.x> --lng <114.x> [--limit <1-50>] [--type <text>] [--json]
    hkpollution watch      [--interval <secs>] [--aqhi <1-11>] [--red-tide] [--radiation]
                           [--once] [--json]
    hkpollution map        --bbox <lat0,lng0,lat1,lng1> [--pid <id>] [--at <iso>] [--json]
    hkpollution about      [--json]
    hkpollution doctor     [--json]
    hkpollution update     [--check] [--force]
    hkpollution mcp
    hkpollution --help | --version

OPTIONS:
    --station <name>   case-insensitive station substring; ambiguous names return all matches
    --near <lat>,<lng> nearest station (or beach) to a Hong Kong coordinate
                       (22.1-22.6, 113.8-114.5); cannot be combined with --station
    --lat <deg>        street-level latitude in Hong Kong (22.1-22.6), with --lng
    --lng <deg>        street-level longitude in Hong Kong (113.8-114.5), with --lat
    --hours <N>        hourly rows kept: 1-24 for `pollutants`, 1-23 for `history`,
                       1-48 for `street` (default 1, 24 for `street`)
    --fail-above <N>   exit 3 when the worst AQHI in the output is N or above (1-11)
    --forecast         add the HKO nine-day forecast to `weather`
    --no-trend         skip the extra history request `summary` makes for the trend
    --bbox <box>       lat0,lng0,lat1,lng1 for `map`, inside Hong Kong
    --pid <id>         pollutant id for `map` (default AQHIBN)
    --at <iso>         timestamp for `map` (default: the current hour)
    --risk <band>      low, moderate, high, very-high, serious (or 1-3, 4-6, 7, 8-10, 11)
    --lang <code>      advice language: en (default), tc, both
    --json             print one JSON object to stdout and nothing else
    --with-weather     add HKO temperature, humidity and warnings to the summary
    --no-cache         bypass the on-disk TTL cache for this run
    --max-age <secs>   override this run's cache TTL; 0 refetches and rewrites
    --refresh          alias for --max-age 0

EXIT CODES:
    0 success, 1 network/upstream/io failure or a failing doctor check,
    2 usage error, 3 --fail-above threshold reached";

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
        "street" => command_street(&args[1..]),
        "summary" => command_summary(&args[1..]),
        "weather" => command_weather(&args[1..]),
        "beach" => command_beach(&args[1..]),
        "river" => command_water(&args[1..], water::Kind::River),
        "marine" => command_water(&args[1..], water::Kind::Marine),
        "red-tide" | "redtide" => command_red_tide(&args[1..]),
        "radiation" => command_radiation(&args[1..]),
        "uv" => command_uv(&args[1..]),
        "stats" => command_stats(&args[1..]),
        "compare" => command_compare(&args[1..]),
        "digest" => command_digest(&args[1..]),
        "recycling" => command_recycling(&args[1..]),
        "watch" => command_watch(&args[1..]),
        "advice" => command_advice(&args[1..]),
        "brief" => command_brief(&args[1..]),
        "map" => command_map(&args[1..]),
        "about" => command_about(&args[1..]),
        "doctor" => command_doctor(&args[1..]),
        "update" => command_update(&args[1..]),
        other => Err(Error::usage(format!("unknown command: {other}"))),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    Aqhi,
    Pollutants,
    Forecast,
    History,
    Street,
    Summary,
    Weather,
    Advice,
    Brief,
    Map,
    About,
    Doctor,
}

impl Command {
    fn name(self) -> &'static str {
        match self {
            Self::Aqhi => "aqhi",
            Self::Pollutants => "pollutants",
            Self::Forecast => "forecast",
            Self::History => "history",
            Self::Street => "street",
            Self::Summary => "summary",
            Self::Weather => "weather",
            Self::Advice => "advice",
            Self::Brief => "brief",
            Self::Map => "map",
            Self::About => "about",
            Self::Doctor => "doctor",
        }
    }

    fn allows_station(self) -> bool {
        matches!(
            self,
            Self::Aqhi | Self::Pollutants | Self::History | Self::Summary
        )
    }

    fn allows_near(self) -> bool {
        matches!(self, Self::Aqhi | Self::History | Self::Summary)
    }

    fn allows_fail_above(self) -> bool {
        matches!(
            self,
            Self::Aqhi | Self::History | Self::Summary | Self::Brief
        )
    }

    fn allows_cache(self) -> bool {
        !matches!(self, Self::Doctor)
    }

    fn max_hours(self) -> Option<u32> {
        match self {
            Self::Pollutants => Some(pollutants::MAX_HOURS),
            Self::History => Some(epd_app::MAX_HOURS),
            Self::Street => Some(praise::MAX_HOURS),
            _ => None,
        }
    }

    fn allows_location(self) -> bool {
        matches!(self, Self::Street | Self::Brief)
    }

    fn allows_advice_flags(self) -> bool {
        matches!(self, Self::Advice)
    }

    fn allows_forecast_flag(self) -> bool {
        matches!(self, Self::Weather)
    }

    fn allows_bbox(self) -> bool {
        matches!(self, Self::Map)
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
    no_cache: bool,
    station: Option<String>,
    near: Option<(f64, f64)>,
    hours: Option<u32>,
    lat: Option<f64>,
    lng: Option<f64>,
    max_age: Option<u64>,
    fail_above: Option<i64>,
    risk: Option<usize>,
    lang: Option<Lang>,
    forecast: bool,
    no_trend: bool,
    bbox: Option<(f64, f64, f64, f64)>,
    pid: Option<String>,
    at: Option<String>,
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
                    "--station is only valid for `aqhi`, `pollutants`, `history` and `summary`, not `{}`",
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
        } else if arg == "--near" || arg.starts_with("--near=") {
            if !command.allows_near() {
                return Err(Error::usage(format!(
                    "--near is only valid for `aqhi`, `history` and `summary`, not `{}`",
                    command.name()
                )));
            }
            let value = match arg.strip_prefix("--near=") {
                Some(inline) => {
                    index += 1;
                    inline.to_string()
                }
                None => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| Error::usage("--near requires <lat>,<lng>"))?;
                    index += 2;
                    value.clone()
                }
            };
            options.near = Some(parse_near(&value)?);
        } else if arg == "--fail-above" || arg.starts_with("--fail-above=") {
            if !command.allows_fail_above() {
                return Err(Error::usage(format!(
                    "--fail-above is only valid for `aqhi`, `history` and `summary`, not `{}`",
                    command.name()
                )));
            }
            let value = match arg.strip_prefix("--fail-above=") {
                Some(inline) => {
                    index += 1;
                    inline.to_string()
                }
                None => {
                    let value = args.get(index + 1).ok_or_else(|| {
                        Error::usage(format!(
                            "--fail-above requires a whole number between 1 and {MAX_AQHI}"
                        ))
                    })?;
                    index += 2;
                    value.clone()
                }
            };
            let threshold: i64 = value.trim().parse().map_err(|_| {
                Error::usage(format!(
                    "--fail-above must be a whole number between 1 and {MAX_AQHI}, got {value:?}"
                ))
            })?;
            if !(1..=MAX_AQHI).contains(&threshold) {
                return Err(Error::usage(format!(
                    "--fail-above must be between 1 and {MAX_AQHI}"
                )));
            }
            options.fail_above = Some(threshold);
        } else if arg == "--forecast" {
            if !command.allows_forecast_flag() {
                return Err(Error::usage(format!(
                    "--forecast is only valid for `weather`, not `{}`",
                    command.name()
                )));
            }
            options.forecast = true;
            index += 1;
        } else if arg == "--no-trend" {
            if command != Command::Summary {
                return Err(Error::usage(format!(
                    "--no-trend is only valid for `summary`, not `{}`",
                    command.name()
                )));
            }
            options.no_trend = true;
            index += 1;
        } else if arg == "--bbox" || arg.starts_with("--bbox=") {
            if !command.allows_bbox() {
                return Err(Error::usage(format!(
                    "--bbox is only valid for `map`, not `{}`",
                    command.name()
                )));
            }
            let value = take_value(args, &mut index, arg, "--bbox")?;
            options.bbox = Some(parse_bbox(&value)?);
        } else if arg == "--pid" || arg.starts_with("--pid=") {
            if !command.allows_bbox() {
                return Err(Error::usage(format!(
                    "--pid is only valid for `map`, not `{}`",
                    command.name()
                )));
            }
            options.pid = Some(take_value(args, &mut index, arg, "--pid")?);
        } else if arg == "--at" || arg.starts_with("--at=") {
            if !command.allows_bbox() {
                return Err(Error::usage(format!(
                    "--at is only valid for `map`, not `{}`",
                    command.name()
                )));
            }
            options.at = Some(take_value(args, &mut index, arg, "--at")?);
        } else if arg == "--risk" || arg.starts_with("--risk=") {
            if !command.allows_advice_flags() {
                return Err(Error::usage(format!(
                    "--risk is only valid for `advice`, not `{}`",
                    command.name()
                )));
            }
            let value = match arg.strip_prefix("--risk=") {
                Some(inline) => {
                    index += 1;
                    inline.to_string()
                }
                None => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| Error::usage("--risk requires a risk band"))?;
                    index += 2;
                    value.clone()
                }
            };
            options.risk = Some(advice::parse_risk(&value)?);
        } else if arg == "--lang" || arg.starts_with("--lang=") {
            if !command.allows_advice_flags() {
                return Err(Error::usage(format!(
                    "--lang is only valid for `advice`, not `{}`",
                    command.name()
                )));
            }
            let value = match arg.strip_prefix("--lang=") {
                Some(inline) => {
                    index += 1;
                    inline.to_string()
                }
                None => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| Error::usage("--lang requires en, tc or both"))?;
                    index += 2;
                    value.clone()
                }
            };
            options.lang = Some(Lang::parse(&value)?);
        } else if arg == "--hours" || arg.starts_with("--hours=") {
            let Some(max_hours) = command.max_hours() else {
                return Err(Error::usage(format!(
                    "--hours is only valid for `pollutants`, `history` and `street`, not `{}`",
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
        } else if arg == "--lat" || arg.starts_with("--lat=") {
            if !command.allows_location() {
                return Err(Error::usage(format!(
                    "--lat is only valid for `street`, not `{}`",
                    command.name()
                )));
            }
            let value = match arg.strip_prefix("--lat=") {
                Some(inline) => {
                    index += 1;
                    inline.to_string()
                }
                None => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| Error::usage("--lat requires a latitude"))?;
                    index += 2;
                    value.clone()
                }
            };
            let lat: f64 = value
                .trim()
                .parse()
                .map_err(|_| Error::usage(format!("--lat must be a number, got {value:?}")))?;
            options.lat = Some(lat);
        } else if arg == "--lng" || arg.starts_with("--lng=") {
            if !command.allows_location() {
                return Err(Error::usage(format!(
                    "--lng is only valid for `street`, not `{}`",
                    command.name()
                )));
            }
            let value = match arg.strip_prefix("--lng=") {
                Some(inline) => {
                    index += 1;
                    inline.to_string()
                }
                None => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| Error::usage("--lng requires a longitude"))?;
                    index += 2;
                    value.clone()
                }
            };
            let lng: f64 = value
                .trim()
                .parse()
                .map_err(|_| Error::usage(format!("--lng must be a number, got {value:?}")))?;
            options.lng = Some(lng);
        } else if arg == "--no-cache" {
            if !command.allows_cache() {
                return Err(Error::usage(format!(
                    "--no-cache is only valid for network commands, not `{}`",
                    command.name()
                )));
            }
            options.no_cache = true;
            index += 1;
        } else if arg == "--refresh" {
            if !command.allows_cache() {
                return Err(Error::usage(format!(
                    "--refresh is only valid for network commands, not `{}`",
                    command.name()
                )));
            }
            options.max_age = Some(0);
            index += 1;
        } else if arg == "--max-age" || arg.starts_with("--max-age=") {
            if !command.allows_cache() {
                return Err(Error::usage(format!(
                    "--max-age is only valid for network commands, not `{}`",
                    command.name()
                )));
            }
            let value = match arg.strip_prefix("--max-age=") {
                Some(inline) => {
                    index += 1;
                    inline.to_string()
                }
                None => {
                    let value = args
                        .get(index + 1)
                        .ok_or_else(|| Error::usage("--max-age requires a number of seconds"))?;
                    index += 2;
                    value.clone()
                }
            };
            let seconds: u64 = value.trim().parse().map_err(|_| {
                Error::usage(format!(
                    "--max-age must be a non-negative number of seconds, got {value:?}"
                ))
            })?;
            options.max_age = Some(seconds);
        } else {
            return Err(Error::usage(format!(
                "unknown argument for `{}`: {arg}",
                command.name()
            )));
        }
    }
    if options.near.is_some() && options.station.is_some() {
        return Err(Error::usage(
            "--near and --station cannot be combined; pick one",
        ));
    }
    Ok(options)
}

/// Parse `--near <lat>,<lng>` and enforce the Hong Kong bounds used by `street`.
fn parse_near(value: &str) -> Result<(f64, f64)> {
    let (lat, lng) = value
        .split_once(',')
        .ok_or_else(|| Error::usage(format!("--near requires <lat>,<lng>, got {value:?}")))?;
    let lat: f64 = lat
        .trim()
        .parse()
        .map_err(|_| Error::usage(format!("--near latitude must be a number, got {lat:?}")))?;
    let lng: f64 = lng
        .trim()
        .parse()
        .map_err(|_| Error::usage(format!("--near longitude must be a number, got {lng:?}")))?;
    if !lat.is_finite() || !(praise::LAT_RANGE.0..=praise::LAT_RANGE.1).contains(&lat) {
        return Err(Error::usage(format!(
            "--near latitude must be between {} and {} (Hong Kong bounds), got {lat}",
            praise::LAT_RANGE.0,
            praise::LAT_RANGE.1
        )));
    }
    if !lng.is_finite() || !(praise::LNG_RANGE.0..=praise::LNG_RANGE.1).contains(&lng) {
        return Err(Error::usage(format!(
            "--near longitude must be between {} and {} (Hong Kong bounds), got {lng}",
            praise::LNG_RANGE.0,
            praise::LNG_RANGE.1
        )));
    }
    Ok((lat, lng))
}

/// Apply the cache flags for this run before any source is contacted.
/// Read the value of `--flag value` or `--flag=value`, advancing the index.
fn take_value(args: &[String], index: &mut usize, arg: &str, flag: &str) -> Result<String> {
    let (name, value) = match arg.split_once('=') {
        Some((name, value)) => (name, Some(value.to_string())),
        None => (arg, None),
    };
    let _ = flag;
    match value {
        Some(value) => {
            *index += 1;
            Ok(value)
        }
        None => {
            let value = args
                .get(*index + 1)
                .ok_or_else(|| Error::usage(format!("{name} requires a value")))?;
            *index += 2;
            Ok(value.clone())
        }
    }
}

/// Parse `--bbox lat0,lng0,lat1,lng1` and enforce the Hong Kong bounds.
fn parse_bbox(value: &str) -> Result<(f64, f64, f64, f64)> {
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err(Error::usage(format!(
            "--bbox requires lat0,lng0,lat1,lng1, got {value:?}"
        )));
    }
    let mut numbers = [0.0_f64; 4];
    for (slot, part) in numbers.iter_mut().zip(parts) {
        *slot = part
            .parse()
            .map_err(|_| Error::usage(format!("--bbox values must be numbers, got {part:?}")))?;
    }
    let [lat0, lng0, lat1, lng1] = numbers;
    if lat0 >= lat1 || lng0 >= lng1 {
        return Err(Error::usage(
            "--bbox needs lat0 < lat1 and lng0 < lng1 (south-west corner first)",
        ));
    }
    for (lat, lng) in [(lat0, lng0), (lat1, lng1)] {
        if !(praise::LAT_RANGE.0..=praise::LAT_RANGE.1).contains(&lat)
            || !(praise::LNG_RANGE.0..=praise::LNG_RANGE.1).contains(&lng)
        {
            return Err(Error::usage(format!(
                "--bbox must lie inside Hong Kong (lat {}-{}, lng {}-{}), got {lat},{lng}",
                praise::LAT_RANGE.0,
                praise::LAT_RANGE.1,
                praise::LNG_RANGE.0,
                praise::LNG_RANGE.1
            )));
        }
    }
    Ok((lat0, lng0, lat1, lng1))
}

fn apply_cache(options: &Options) {
    http::configure_cache(options.no_cache, options.max_age);
}

/// Exit 3 after the normal output when `--fail-above` is reached.
fn threshold_exit(fail_above: Option<i64>, worst_aqhi: Option<i64>) {
    if let Some(threshold) = fail_above {
        if at_or_above(worst_aqhi, threshold) {
            exit(3);
        }
    }
}

fn command_update(args: &[String]) -> Result<()> {
    let mut check = false;
    let mut force = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            "--force" => force = true,
            other => {
                return Err(Error::usage(format!(
                    "unknown option for `update`: {other}"
                )));
            }
        }
    }
    update::run(check, force)
}

fn command_aqhi(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Aqhi)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    apply_cache(&options);
    let near = options.near.map(|(lat, lng)| nearest_station(lat, lng));
    let mut report = aqhi::fetch()?;
    if let Some((station, _)) = near {
        aqhi::retain_stations(&mut report.stations, &[station.to_string()]);
    } else if let Some(query) = &options.station {
        let names = resolve_station(query)?;
        aqhi::retain_stations(&mut report.stations, &names);
    }
    let worst = report
        .stations
        .iter()
        .filter_map(|reading| reading.aqhi)
        .max();
    if options.json {
        print_json(&report)?;
    } else {
        print_aqhi(&report, near);
    }
    threshold_exit(options.fail_above, worst);
    Ok(())
}

fn command_pollutants(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Pollutants)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    apply_cache(&options);
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
    apply_cache(&options);
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
    apply_cache(&options);
    let near = options.near.map(|(lat, lng)| nearest_station(lat, lng));
    let query = match near {
        Some((station, _)) => Some(station.to_string()),
        None => options.station.clone(),
    };
    let hours = options.hours.unwrap_or(1);
    let report = epd_app::fetch_history(query.as_deref(), hours)
        .or_else(|error| aqhi24::history_report(query.as_deref(), hours).map_err(|_| error))?;
    let worst = report
        .stations
        .iter()
        .flat_map(|station| station.series.iter())
        .filter_map(|reading| reading.aqhi)
        .max();
    if options.json {
        print_json(&report)?;
    } else {
        print_history(&report, near);
    }
    threshold_exit(options.fail_above, worst);
    Ok(())
}

fn command_street(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Street)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    let (Some(lat), Some(lng)) = (options.lat, options.lng) else {
        return Err(Error::usage("`street` requires --lat and --lng together"));
    };
    apply_cache(&options);
    let report = praise::fetch(lat, lng, options.hours.unwrap_or(praise::DEFAULT_HOURS))?;
    if options.json {
        print_json(&report)
    } else {
        print_street(&report);
        Ok(())
    }
}

fn command_summary(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Summary)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    apply_cache(&options);
    let near = options.near.map(|(lat, lng)| nearest_station(lat, lng));
    let names = match (near, &options.station) {
        (Some((station, _)), _) => Some(vec![station.to_string()]),
        (None, Some(query)) => Some(resolve_station(query)?),
        (None, None) => None,
    };
    let mut report = aqhi::summary(options.with_weather)?;
    if let Some(names) = &names {
        report.stations.retain(|reading| {
            names
                .iter()
                .any(|name| name.eq_ignore_ascii_case(&reading.station))
        });
        let rolled = rollup(&report.stations);
        report.worst = rolled.worst;
        report.mean_aqhi = rolled.mean_aqhi;
        report.risk_counts = rolled.risk_counts;
    }
    let worst = report
        .stations
        .iter()
        .filter_map(|reading| reading.aqhi)
        .max();
    if !options.no_trend {
        let station = report
            .worst
            .as_ref()
            .map(|reading| reading.station.clone())
            .or_else(|| {
                report
                    .stations
                    .first()
                    .map(|reading| reading.station.clone())
            });
        if let Some(station) = station {
            report.trend = aqhi::trend(&station, TREND_HOURS).ok();
        }
    }
    if options.json {
        print_json(&report)?;
    } else {
        print_summary(&report);
    }
    threshold_exit(options.fail_above, worst);
    Ok(())
}

fn command_weather(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Weather)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    apply_cache(&options);
    let report = weather::fetch()?;
    let forecast = if options.forecast {
        Some(weather::fetch_forecast()?)
    } else {
        None
    };
    let ultraviolet = uv::fetch().ok();
    if options.json {
        let mut payload = serde_json::to_value(&report)?;
        if let Some(object) = payload.as_object_mut() {
            if let Some(uv) = &ultraviolet {
                object.insert("uv".to_string(), serde_json::to_value(uv)?);
            }
            if let Some(forecast) = &forecast {
                object.insert("forecast".to_string(), serde_json::to_value(forecast)?);
            }
        }
        print_json(&payload)
    } else {
        print_weather(&report);
        if let Some(uv) = &ultraviolet {
            println!("uv: {:.1} ({}) at {}", uv.index, uv.band, uv.time);
        }
        if let Some(forecast) = &forecast {
            print_weather_forecast(forecast);
        }
        Ok(())
    }
}

fn command_beach(args: &[String]) -> Result<()> {
    let mut json = false;
    let mut name: Option<String> = None;
    let mut district: Option<String> = None;
    let mut grade: Option<String> = None;
    let mut near: Option<(f64, f64)> = None;
    let mut history = 0_u32;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--no-cache" => {
                http::configure_cache(true, None);
                index += 1;
            }
            "--refresh" => {
                http::configure_cache(false, Some(0));
                index += 1;
            }
            "--max-age" | "--name" | "--district" | "--grade" | "--near" | "--history" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| Error::usage(format!("{arg} requires a value")))?
                    .clone();
                match arg {
                    "--max-age" => {
                        let seconds: u64 = value.trim().parse().map_err(|_| {
                            Error::usage(format!(
                                "--max-age must be a non-negative number of seconds, got {value:?}"
                            ))
                        })?;
                        http::configure_cache(false, Some(seconds));
                    }
                    "--name" => name = Some(value),
                    "--district" => district = Some(value),
                    "--near" => near = Some(parse_near(&value)?),
                    "--history" => {
                        history = value.trim().parse().map_err(|_| {
                            Error::usage(format!("--history must be a whole number, got {value:?}"))
                        })?;
                    }
                    _ => grade = Some(value),
                }
                index += 2;
            }
            other => {
                return Err(Error::usage(format!(
                    "unknown argument for `beach`: {other}"
                )));
            }
        }
    }
    let mut report = beach::fetch_with_history(history)?;
    if let Some(query) = &name {
        beach::retain_query(&mut report, query);
    }
    if let Some(query) = &district {
        let needle = query.to_lowercase();
        report
            .beaches
            .retain(|item| item.district.to_lowercase().contains(&needle));
        report.count = report.beaches.len();
    }
    if let Some(band) = &grade {
        beach::retain_grade(&mut report, band)?;
    }
    if let Some((lat, lng)) = near {
        beach::sort_by_distance(&mut report, lat, lng);
    }
    if json {
        print_json(&report)?;
    } else if report.beaches.is_empty() {
        println!("No beaches matched.");
    } else {
        let mut headers = vec!["DISTRICT", "BEACH", "GRADE", "STATUS"];
        if near.is_some() {
            headers.push("DIST");
        }
        headers.extend(["E. COLI", "SAMPLED"]);
        let rows: Vec<Vec<String>> = report
            .beaches
            .iter()
            .map(|item| {
                let mut row = vec![
                    item.district.clone(),
                    item.name.clone(),
                    item.grade_name.clone(),
                    item.status.clone(),
                ];
                if near.is_some() {
                    row.push(match item.distance_km {
                        Some(distance) => format!("{distance:.1} km"),
                        None => "-".to_string(),
                    });
                }
                row.push(match &item.ecoli {
                    Some(ecoli) => ecoli.latest_per_100ml.to_string(),
                    None => "-".to_string(),
                });
                row.push(match &item.ecoli {
                    Some(ecoli) => ecoli.latest_sampled_on.clone(),
                    None => "-".to_string(),
                });
                row
            })
            .collect();
        print_table(&headers, &rows);
        println!(
            "\n{} of {} beaches graded, {} open for swimming",
            report.graded, report.count, report.open
        );
        if let Some(source) = &report.ecoli_source {
            let year = source
                .rsplit('/')
                .next()
                .and_then(|file| file.strip_prefix("beach-historical-data-"))
                .and_then(|rest| rest.get(0..4))
                .unwrap_or("latest");
            println!("E. coli: EPD beach monitoring, {year} published sampling file (per 100 mL)");
        }
        if let Some(error) = &report.ecoli_error {
            println!("E. coli unavailable: {error}");
        }
        println!("\nsource: {}", report.source);
    }
    Ok(())
}

fn command_advice(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Advice)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    apply_cache(&options);
    let lang = options.lang.unwrap_or(Lang::En);
    let report = advice::fetch(lang, options.risk)?;
    if options.json {
        print_json(&report)
    } else {
        print_advice(&report);
        Ok(())
    }
}

fn command_brief(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Brief)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    let (Some(lat), Some(lng)) = (options.lat, options.lng) else {
        return Err(Error::usage("`brief` requires --lat and --lng together"));
    };
    apply_cache(&options);
    let report = brief::build(lat, lng)?;
    if options.json {
        print_json(&report)?;
    } else {
        print_brief(&report);
    }
    if report.partial {
        eprintln!("hkpollution: warning: partial report; see the *_error fields in --json output");
    }
    threshold_exit(options.fail_above, brief::worst_aqhi(&report));
    Ok(())
}

fn command_map(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Map)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    let Some((lat0, lng0, lat1, lng1)) = options.bbox else {
        return Err(Error::usage("`map` requires --bbox <lat0,lng0,lat1,lng1>"));
    };
    apply_cache(&options);
    let pollutant = options.pid.as_deref().unwrap_or("AQHIBN");
    let report = praise::spatial_tile(lat0, lng0, lat1, lng1, pollutant, options.at.as_deref())?;
    if options.json {
        print_json(&report)
    } else {
        print_map(&report);
        Ok(())
    }
}

fn command_about(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::About)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    apply_cache(&options);
    let sections = epd_app::fetch_abouts()?;
    if options.json {
        let payload = serde_json::json!({
            "sections": sections,
            "source": epd_app::SOURCE,
            "fetched_at": hongkong_pollution_skill::now_hkt_iso8601(),
        });
        print_json(&payload)
    } else {
        print_about(&sections);
        Ok(())
    }
}

fn command_doctor(args: &[String]) -> Result<()> {
    let options = parse_options(args, Command::Doctor)?;
    if options.help {
        println!("{USAGE}");
        return Ok(());
    }
    let report = doctor::run(&doctor::default_probes());
    if options.json {
        print_json(&report)?;
    } else {
        print_doctor(&report);
    }
    if !report.healthy() {
        exit(1);
    }
    Ok(())
}

fn print_advice(report: &advice::AdviceReport) {
    for (index, band) in report.bands.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("{} (AQHI {})", band.risk, band.aqhi_range);
        if let Some(row) = &band.advice {
            print_advice_row(row);
        }
        if let Some(row) = &band.advice_zh {
            println!("  [zh]");
            print_advice_row(row);
        }
    }
    println!();
    println!(
        "source: {} health advice (fetched {})",
        report.source, report.fetched_at
    );
}

fn print_advice_row(row: &advice::AdviceRow) {
    println!("  heart and respiratory  {}", row.heart_and_respiratory);
    println!("  children and elderly   {}", row.children_and_elderly);
    println!("  outdoor workers        {}", row.outdoor_workers);
    println!("  general public         {}", row.general_public);
}

fn print_brief(report: &brief::BriefReport) {
    let place = report
        .location
        .place
        .as_deref()
        .unwrap_or("(place name unavailable)");
    println!(
        "place:    {place} ({:.4}, {:.4})",
        report.location.lat, report.location.lng
    );
    match (&report.street, &report.street_error) {
        (Some(street), _) => println!(
            "street:   AQHI {} · added risk {}% · {}",
            format_i64(street.aqhi),
            format_f64(street.added_health_risk),
            pretty_time(&street.time)
        ),
        (None, Some(error)) => println!("street:   unavailable ({error})"),
        (None, None) => {}
    }
    match (&report.station, &report.station_error) {
        (Some(station), _) => println!(
            "station:  {} ({:.1} km away) AQHI {} · {} · {}",
            station.name,
            station.distance_km,
            format_i64(station.aqhi),
            station.risk.as_deref().unwrap_or("-"),
            station
                .time
                .as_deref()
                .map_or_else(|| "-".to_string(), pretty_time)
        ),
        (None, Some(error)) => println!("station:  unavailable ({error})"),
        (None, None) => {}
    }
    if let Some(weather) = &report.weather {
        let warnings = if weather.warnings.is_empty() {
            "no warnings".to_string()
        } else {
            weather.warnings.join("; ")
        };
        println!(
            "weather:  {} C · {} % humidity · {warnings}",
            format_f64(weather.temperature),
            format_i64(weather.humidity)
        );
    }
    if let Some(uv) = &report.uv {
        println!(
            "uv:       {:.1} ({}) at {}",
            uv.index,
            uv.band,
            pretty_time(&uv.time)
        );
    }
    match (&report.beach, &report.beach_error) {
        (Some(beach), _) => println!(
            "beach:    {} ({:.1} km) {} · {}{}",
            beach.name,
            beach.distance_km,
            beach.grade_name,
            beach.status,
            match (&beach.ecoli_latest_per_100ml, &beach.ecoli_sampled_on) {
                (Some(count), Some(date)) => format!(" · E. coli {count}/100 mL on {date}"),
                _ => String::new(),
            }
        ),
        (None, Some(error)) => println!("beach:    unavailable ({error})"),
        (None, None) => {}
    }
    match (&report.red_tide, &report.red_tide_error) {
        (Some(tide), _) => println!(
            "red tide: {} sighting(s) within 10 km, {} active{}",
            tide.sightings_within_10km,
            tide.active_within_10km,
            match (&tide.nearest_location, tide.nearest_km) {
                (Some(location), Some(km)) => format!(
                    " · nearest {location} ({km:.1} km){} ",
                    if tide.nearest_active { " ACTIVE" } else { "" }
                ),
                _ => String::new(),
            }
        ),
        (None, Some(error)) => println!("red tide: unavailable ({error})"),
        (None, None) => {}
    }
    if let Some(advice) = &report.advice {
        println!();
        println!("advice for {} (AQHI {}):", advice.risk, advice.aqhi_range);
        println!("  general public         {}", advice.row.general_public);
        println!(
            "  heart and respiratory  {}",
            advice.row.heart_and_respiratory
        );
        println!(
            "  children and elderly   {}",
            advice.row.children_and_elderly
        );
        println!("  outdoor workers        {}", advice.row.outdoor_workers);
    }
    println!();
    println!("source: {} (fetched {})", report.source, report.fetched_at);
}

fn print_weather_forecast(report: &weather::ForecastReport) {
    if let Some(situation) = &report.situation {
        println!("{situation}");
        println!();
    }
    let rows: Vec<Vec<String>> = report
        .days
        .iter()
        .take(5)
        .map(|day| {
            vec![
                day.date.clone(),
                day.week.clone(),
                day.weather.clone().unwrap_or_else(|| "-".to_string()),
                match (day.min_temp, day.max_temp) {
                    (Some(min), Some(max)) => format!("{min}-{max} C"),
                    _ => "-".to_string(),
                },
                match (day.min_humidity, day.max_humidity) {
                    (Some(min), Some(max)) => format!("{min}-{max} %"),
                    _ => "-".to_string(),
                },
                day.psr.clone().unwrap_or_else(|| "-".to_string()),
            ]
        })
        .collect();
    print_table(
        &["Date", "Day", "Weather", "Temp", "Humidity", "Rain"],
        &rows,
    );
    println!();
    println!("forecast source: HKO nine-day forecast");
}

fn print_map(report: &praise::MapReport) {
    let (lat0, lng0, lat1, lng1) = report.coverage;
    println!("pollutant: {}", report.pollutant);
    println!("time:      {}", report.ts.as_deref().unwrap_or("-"));
    println!("coverage:  {lat0:.4},{lng0:.4} to {lat1:.4},{lng1:.4}");
    println!("tile:      {}", report.url);
    println!();
    println!("note: this is a link to a generated PNG, not data; the filename is ephemeral");
    println!("source: {} (fetched {})", report.source, report.fetched_at);
}

fn print_about(sections: &[hongkong_pollution_skill::parse::AboutSection]) {
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!("{}", section.title);
        if let Some(url) = &section.url {
            println!("  {url}");
        }
        if !section.content.is_empty() {
            println!("  {}", section.content);
        }
    }
    println!();
    println!("source: EPD app API about pages");
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    let encoded = serde_json::to_string(value)?;
    println!("{encoded}");
    Ok(())
}

fn print_aqhi(report: &AqhiReport, near: Option<(&str, f64)>) {
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
    print_near(near);
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

fn print_history(report: &HistoryReport, near: Option<(&str, f64)>) {
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
    print_near(near);
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

fn print_street(report: &StreetReport) {
    let rows: Vec<Vec<String>> = report
        .series
        .iter()
        .map(|reading| {
            vec![
                pretty_time(&reading.time),
                format_i64(reading.aqhi),
                format_f64(reading.added_health_risk),
                format_f64(reading.no2),
                format_f64(reading.o3),
                format_f64(reading.pm10),
                format_f64(reading.pm25),
                format_f64(reading.so2),
            ]
        })
        .collect();
    print_table(
        &["Time", "AQHI", "%AR", "NO2", "O3", "PM10", "PM2.5", "SO2"],
        &rows,
    );
    println!();
    println!(
        "source: PRAISE-HK street-level model © HKUST Institute for the Environment (fetched {})",
        report.fetched_at
    );
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
    if let Some(trend) = &report.trend {
        println!();
        let unit = if trend.delta.abs() == 1 {
            "point"
        } else {
            "points"
        };
        println!(
            "trend: {} {} {} over {} h at {} ({} -> {})",
            trend.direction,
            trend.delta.abs(),
            unit,
            trend.hours,
            trend.station,
            trend.from,
            trend.to
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

fn print_doctor(report: &doctor::Report) {
    let rows: Vec<Vec<String>> = report
        .checks
        .iter()
        .map(|check| {
            vec![
                check.source.clone(),
                check.status.clone(),
                check.ms.to_string(),
                check.detail.clone(),
            ]
        })
        .collect();
    print_table(&["SOURCE", "STATUS", "MS", "DETAIL"], &rows);
    println!();
    println!("version: {}", report.version);
    match &report.cache.dir {
        Some(dir) => println!("cache: {dir}"),
        None => println!("cache: unavailable"),
    };
    println!(
        "cache entries: {} ({} bytes)",
        report.cache.entries, report.cache.bytes
    );
    println!(
        "summary: {} ok, {} stale, {} failed",
        report.summary.ok, report.summary.stale, report.summary.failed
    );
}

fn print_near(near: Option<(&str, f64)>) {
    if let Some((station, distance)) = near {
        println!("station: {station} ({distance:.1} km away)");
    }
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

/// Handle the shared cache flags; `true` when `arg` was one of them.
fn cache_flag(arg: &str, args: &[String], index: &mut usize) -> Result<bool> {
    match arg {
        "--no-cache" => {
            http::configure_cache(true, None);
            *index += 1;
            Ok(true)
        }
        "--refresh" => {
            http::configure_cache(false, Some(0));
            *index += 1;
            Ok(true)
        }
        _ if arg == "--max-age" || arg.starts_with("--max-age=") => {
            let value = take_value(args, index, arg, "--max-age")?;
            let seconds: u64 = value.trim().parse().map_err(|_| {
                Error::usage(format!(
                    "--max-age must be a non-negative number of seconds, got {value:?}"
                ))
            })?;
            http::configure_cache(false, Some(seconds));
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn command_water(args: &[String], kind: water::Kind) -> Result<()> {
    let mut json = false;
    let mut months = 6_u32;
    let mut zone: Option<String> = None;
    let mut station: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if cache_flag(arg, args, &mut index)? {
            continue;
        }
        match arg {
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--months" | "--zone" | "--station" => {
                let value = take_value(args, &mut index, arg, arg)?;
                match arg {
                    "--months" => {
                        months = value.trim().parse().map_err(|_| {
                            Error::usage(format!("--months must be a whole number, got {value:?}"))
                        })?;
                    }
                    "--zone" => zone = Some(value),
                    _ => station = Some(value),
                }
            }
            other => {
                return Err(Error::usage(format!(
                    "unknown argument for `{}`: {other}",
                    kind.name()
                )));
            }
        }
    }
    let mut report = water::fetch(kind, months)?;
    if let Some(zone) = &zone {
        water::retain_zone(&mut report, zone);
    }
    if let Some(query) = &station {
        water::retain_query(&mut report, query);
    }
    if json {
        return print_json(&report);
    }
    if report.stations.is_empty() {
        println!("No stations matched.");
        return Ok(());
    }
    let headers: Vec<&str> = if kind == water::Kind::River {
        vec!["ZONE", "RIVER", "STATION", "MONTH", "DO mg/L", "BOD5 mg/L"]
    } else {
        vec!["ZONE", "STATION", "MONTH", "DO mg/L", "BOD5 mg/L"]
    };
    let rows: Vec<Vec<String>> = report
        .stations
        .iter()
        .map(|entry| {
            let mut row = vec![entry.control_zone.clone()];
            if kind == water::Kind::River {
                row.push(entry.watercourse.clone().unwrap_or_else(|| "-".to_string()));
            }
            row.extend([
                entry.station.clone(),
                entry.latest.month.clone(),
                number(entry.latest.dissolved_oxygen_mg_l),
                number(entry.latest.bod5_mg_l),
            ]);
            row
        })
        .collect();
    print_table(&headers, &rows);
    println!(
        "\n{} stations, newest sampling month {}",
        report.count,
        report.latest_month.as_deref().unwrap_or("unknown")
    );
    println!("EPD publishes these monthly, so they are not live readings.");
    println!("\nsource: {}", report.source);
    Ok(())
}

/// Shorten a long cell so a table stays readable on a terminal.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut shortened: String = text.chars().take(max.saturating_sub(1)).collect();
    shortened.push('…');
    shortened
}

fn number(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_string(), |value| format!("{value:.1}"))
}

fn command_red_tide(args: &[String]) -> Result<()> {
    let mut json = false;
    let mut active = false;
    let mut days = redtide::DEFAULT_DAYS;
    let mut near: Option<(f64, f64)> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if cache_flag(arg, args, &mut index)? {
            continue;
        }
        match arg {
            "--json" => {
                json = true;
                index += 1;
            }
            "--active" => {
                active = true;
                index += 1;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--days" => {
                let value = take_value(args, &mut index, arg, "--days")?;
                days = value.trim().parse().map_err(|_| {
                    Error::usage(format!("--days must be a whole number, got {value:?}"))
                })?;
            }
            "--near" => {
                let value = take_value(args, &mut index, arg, "--near")?;
                near = Some(parse_near(&value)?);
            }
            other => {
                return Err(Error::usage(format!(
                    "unknown argument for `red-tide`: {other}"
                )));
            }
        }
    }
    let mut report = redtide::fetch(days)?;
    if active {
        redtide::retain_active(&mut report);
    }
    if let Some((lat, lng)) = near {
        redtide::sort_by_distance(&mut report, lat, lng);
    }
    if json {
        return print_json(&report);
    }
    if report.sightings.is_empty() {
        println!(
            "No red tide sightings reported in the last {} days.",
            report.days
        );
        return Ok(());
    }
    let rows: Vec<Vec<String>> = report
        .sightings
        .iter()
        .map(|sighting| {
            vec![
                sighting.reported_on.clone(),
                sighting.location.clone(),
                sighting.species.clone(),
                if sighting.active { "ACTIVE" } else { "" }.to_string(),
                sighting
                    .dissipated_on
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
                match sighting.distance_km {
                    Some(distance) => format!("{distance:.1} km"),
                    None => "-".to_string(),
                },
            ]
        })
        .collect();
    print_table(
        &[
            "REPORTED",
            "LOCATION",
            "SPECIES",
            "STATUS",
            "DISSIPATED",
            "DIST",
        ],
        &rows,
    );
    println!(
        "\n{} sighting(s), {} active, window {} days",
        report.count, report.active, report.days
    );
    println!("\nsource: {}", report.source);
    Ok(())
}

fn command_radiation(args: &[String]) -> Result<()> {
    let mut json = false;
    let mut station: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if cache_flag(arg, args, &mut index)? {
            continue;
        }
        match arg {
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--station" => station = Some(take_value(args, &mut index, arg, "--station")?),
            other => {
                return Err(Error::usage(format!(
                    "unknown argument for `radiation`: {other}"
                )));
            }
        }
    }
    let mut report = radiation::fetch()?;
    if let Some(query) = &station {
        radiation::retain_query(&mut report, query);
    }
    if json {
        return print_json(&report);
    }
    if report.stations.is_empty() {
        println!("No radiation station matched.");
        return Ok(());
    }
    let rows: Vec<Vec<String>> = report
        .stations
        .iter()
        .map(|reading| {
            vec![
                reading.station.clone(),
                format!("{:.2}", reading.microsievert_per_hour),
                match reading.relative_to_median {
                    Some(ratio) => format!("{ratio:.2}x"),
                    None => "-".to_string(),
                },
                reading.level.clone(),
                reading.time.clone(),
            ]
        })
        .collect();
    print_table(&["STATION", "µSv/H", "VS MEDIAN", "LEVEL", "TIME"], &rows);
    println!(
        "\n{} stations, network median {:.2} µSv/h, observed {}",
        report.count, report.median_microsievert_per_hour, report.observed_at
    );
    println!("`elevated` means more than twice the network median, not a health threshold.");
    println!("\nsource: {}", report.source);
    Ok(())
}

fn command_uv(args: &[String]) -> Result<()> {
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if cache_flag(arg, args, &mut index)? {
            continue;
        }
        match arg {
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => {
                return Err(Error::usage(format!("unknown argument for `uv`: {other}")));
            }
        }
    }
    let report = uv::fetch()?;
    if json {
        return print_json(&report);
    }
    println!(
        "UV index {:.1} ({}) at {}\n\nsource: {}",
        report.index, report.band, report.time, report.source
    );
    Ok(())
}

fn command_stats(args: &[String]) -> Result<()> {
    let mut json = false;
    let mut days = 30_u32;
    let mut threshold = stats::DEFAULT_THRESHOLD;
    let mut station: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if cache_flag(arg, args, &mut index)? {
            continue;
        }
        match arg {
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--days" | "--threshold" | "--station" => {
                let value = take_value(args, &mut index, arg, arg)?;
                match arg {
                    "--days" => {
                        days = value.trim().parse().map_err(|_| {
                            Error::usage(format!("--days must be a whole number, got {value:?}"))
                        })?;
                    }
                    "--threshold" => {
                        threshold = value.trim().parse().map_err(|_| {
                            Error::usage(format!(
                                "--threshold must be a whole number, got {value:?}"
                            ))
                        })?;
                    }
                    _ => station = Some(value),
                }
            }
            other => {
                return Err(Error::usage(format!(
                    "unknown argument for `stats`: {other}"
                )));
            }
        }
    }
    let report = stats::build(days, station.as_deref(), threshold)?;
    if json {
        return print_json(&report);
    }
    let rows: Vec<Vec<String>> = report
        .stations
        .iter()
        .map(|entry| {
            vec![
                entry.station.clone(),
                entry.category.clone(),
                entry.samples.to_string(),
                entry.min.to_string(),
                entry.max.to_string(),
                format!("{:.1}", entry.mean),
                entry.hours_at_or_above.to_string(),
                entry.last_seen.clone(),
            ]
        })
        .collect();
    print_table(
        &[
            "STATION", "CATEGORY", "SAMPLES", "MIN", "MAX", "MEAN", ">=T", "LAST",
        ],
        &rows,
    );
    println!(
        "\n{} observations over {} days, threshold AQHI {}{}",
        report.observations,
        report.days,
        report.threshold,
        report
            .coverage_from
            .as_ref()
            .map(|from| format!(", coverage from {from}"))
            .unwrap_or_default()
    );
    println!(
        "The local log grows with every call; the documented 24-hour feed fills the first run."
    );
    println!("\nsource: {}", report.source);
    Ok(())
}

fn command_compare(args: &[String]) -> Result<()> {
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if cache_flag(arg, args, &mut index)? {
            continue;
        }
        match arg {
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => {
                return Err(Error::usage(format!(
                    "unknown argument for `compare`: {other}"
                )));
            }
        }
    }
    let report = stats::compare()?;
    if json {
        return print_json(&report);
    }
    let rows: Vec<Vec<String>> = report
        .stations
        .iter()
        .map(|row| {
            vec![
                row.station.clone(),
                row.category.clone(),
                row.aqhi
                    .map_or_else(|| "-".to_string(), |aqhi| aqhi.to_string()),
                row.risk.clone().unwrap_or_else(|| "-".to_string()),
                match row.vs_category_mean {
                    Some(delta) => format!("{delta:+.1}"),
                    None => "-".to_string(),
                },
            ]
        })
        .collect();
    print_table(
        &["STATION", "CATEGORY", "AQHI", "RISK", "VS CATEGORY"],
        &rows,
    );
    println!(
        "\nnetwork mean {}, general {}, roadside {}",
        number(report.network_mean),
        number(report.general_mean),
        number(report.roadside_mean)
    );
    println!("Roadside stations sit in traffic; compare them with roadside, not with Tap Mun.");
    println!("\nsource: {}", report.source);
    Ok(())
}

fn command_digest(args: &[String]) -> Result<()> {
    let mut json = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if cache_flag(arg, args, &mut index)? {
            continue;
        }
        match arg {
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => {
                return Err(Error::usage(format!(
                    "unknown argument for `digest`: {other}"
                )));
            }
        }
    }
    let report = digest::build();
    if json {
        return print_json(&report);
    }
    for line in &report.lines {
        match &line.error {
            Some(error) => println!("{:<10} unavailable: {error}", line.source),
            None => println!("{:<10} {}", line.source, line.summary),
        }
    }
    println!(
        "\n{} of {} sources ok (fetched {})",
        report.ok,
        report.lines.len(),
        report.fetched_at
    );
    Ok(())
}

fn command_recycling(args: &[String]) -> Result<()> {
    let mut json = false;
    let mut lat: Option<f64> = None;
    let mut lng: Option<f64> = None;
    let mut limit = recycling::DEFAULT_LIMIT;
    let mut waste: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if cache_flag(arg, args, &mut index)? {
            continue;
        }
        match arg {
            "--json" => {
                json = true;
                index += 1;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--lat" | "--lng" | "--limit" | "--type" => {
                let value = take_value(args, &mut index, arg, arg)?;
                match arg {
                    "--lat" => {
                        lat = Some(value.trim().parse().map_err(|_| {
                            Error::usage(format!("--lat must be a number, got {value:?}"))
                        })?);
                    }
                    "--lng" => {
                        lng = Some(value.trim().parse().map_err(|_| {
                            Error::usage(format!("--lng must be a number, got {value:?}"))
                        })?);
                    }
                    "--limit" => {
                        limit = value.trim().parse().map_err(|_| {
                            Error::usage(format!("--limit must be a whole number, got {value:?}"))
                        })?;
                    }
                    _ => waste = Some(value),
                }
            }
            other => {
                return Err(Error::usage(format!(
                    "unknown argument for `recycling`: {other}"
                )));
            }
        }
    }
    let (Some(lat), Some(lng)) = (lat, lng) else {
        return Err(Error::usage(
            "`recycling` requires --lat and --lng together",
        ));
    };
    let report = recycling::near(lat, lng, limit, waste.as_deref())?;
    if json {
        return print_json(&report);
    }
    let rows: Vec<Vec<String>> = report
        .points
        .iter()
        .map(|point| {
            vec![
                point.name.clone(),
                point.district.clone(),
                format!("{:.1} km", point.distance_km),
                truncate(&point.waste_types.join(", "), 48),
                truncate(point.opening_hours.as_deref().unwrap_or("-"), 40),
            ]
        })
        .collect();
    print_table(&["POINT", "DISTRICT", "DIST", "ACCEPTS", "HOURS"], &rows);
    println!(
        "\n{} nearest point(s){}",
        report.count,
        report
            .waste_type
            .as_ref()
            .map(|kind| format!(" accepting {kind}"))
            .unwrap_or_default()
    );
    println!("\nsource: {}", report.source);
    Ok(())
}

fn command_watch(args: &[String]) -> Result<()> {
    let mut json = false;
    let mut once = false;
    let mut interval = 300_u64;
    let mut thresholds = watch::Thresholds::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if cache_flag(arg, args, &mut index)? {
            continue;
        }
        match arg {
            "--json" => {
                json = true;
                index += 1;
            }
            "--once" => {
                once = true;
                index += 1;
            }
            "--red-tide" => {
                thresholds.red_tide = true;
                index += 1;
            }
            "--radiation" => {
                thresholds.radiation = true;
                index += 1;
            }
            "--help" | "-h" => {
                println!("{USAGE}");
                return Ok(());
            }
            "--interval" | "--aqhi" => {
                let value = take_value(args, &mut index, arg, arg)?;
                match arg {
                    "--interval" => {
                        interval = value.trim().parse().map_err(|_| {
                            Error::usage(format!(
                                "--interval must be a whole number of seconds, got {value:?}"
                            ))
                        })?;
                    }
                    _ => {
                        thresholds.aqhi = value.trim().parse().map_err(|_| {
                            Error::usage(format!("--aqhi must be a whole number, got {value:?}"))
                        })?;
                    }
                }
            }
            other => {
                return Err(Error::usage(format!(
                    "unknown argument for `watch`: {other}"
                )));
            }
        }
    }
    if interval == 0 {
        return Err(Error::usage("--interval must be at least 1 second"));
    }
    loop {
        let check = watch::check(&thresholds)?;
        if json {
            print_json(&check)?;
        } else {
            let worst = check
                .worst_aqhi
                .as_ref()
                .map(|worst| format!("AQHI {} at {}", worst.aqhi, worst.station))
                .unwrap_or_else(|| "AQHI unavailable".to_string());
            if check.alerts.is_empty() {
                println!("{} ok · {worst}", check.at);
            } else {
                for alert in &check.alerts {
                    println!("{} ALERT {}: {}", check.at, alert.source, alert.message);
                }
            }
            for error in &check.errors {
                println!("{} warning: {error}", check.at);
            }
        }
        if once {
            if check.triggered() {
                exit(3);
            }
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(interval));
    }
}
