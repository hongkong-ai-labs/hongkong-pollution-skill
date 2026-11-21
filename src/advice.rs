//! AQHI health advice: the official EPD precautionary actions per risk band.
//!
//! The English wording is EPD's published table (aqhi.gov.hk, "Recommended
//! Precautionary Actions") and is compiled in, so English advice keeps working
//! when the app API is down. The Traditional Chinese wording comes from the
//! app API (`/api/v2/health_advices`) and is only fetched for `--lang tc` or
//! `--lang both`.

use crate::error::{Error, Result};
use crate::http;
use crate::now_hkt_iso8601;
use crate::parse;
use serde::Serialize;

/// App API endpoint carrying the Traditional Chinese advice.
pub const HEALTH_ADVICES_URL: &str = "https://app.epd-asmg.gov.hk/api/v2/health_advices";

/// Source label for the compiled-in English table.
pub const SOURCE_EN: &str = "EPD";

/// Source label for the upstream Traditional Chinese text.
pub const SOURCE_ZH: &str = "EPD app API";

/// Cache TTL for the advice payload; it changes a few times a year at most.
const CACHE_TTL_SECS: u64 = 86_400;

/// Which language to report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// Compiled-in official English table.
    En,
    /// Upstream Traditional Chinese text.
    Tc,
    /// Both, with the Chinese text alongside.
    Both,
}

impl Lang {
    /// Parse the `--lang` value.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Usage`] for anything other than `en`, `tc` or `both`.
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "en" | "en-gb" | "en-us" => Ok(Self::En),
            "tc" | "zh" | "zh-hk" | "zh-hant" => Ok(Self::Tc),
            "both" => Ok(Self::Both),
            other => Err(Error::usage(format!(
                "--lang must be en, tc or both, got {other:?}"
            ))),
        }
    }

    /// Stable label used in the JSON payload.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Tc => "tc",
            Self::Both => "both",
        }
    }

    fn wants_english(self) -> bool {
        matches!(self, Self::En | Self::Both)
    }

    fn wants_chinese(self) -> bool {
        matches!(self, Self::Tc | Self::Both)
    }
}

/// The four audience rows EPD publishes for one band.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdviceRow {
    /// People with existing heart or respiratory illnesses.
    pub heart_and_respiratory: String,
    /// Children and the elderly.
    pub children_and_elderly: String,
    /// Outdoor workers.
    pub outdoor_workers: String,
    /// The general public.
    pub general_public: String,
}

/// One band's advice.
#[derive(Debug, Clone, Serialize)]
pub struct AdviceBand {
    /// Risk label, for example `Moderate`.
    pub risk: &'static str,
    /// AQHI range, for example `4-6`.
    pub aqhi_range: &'static str,
    /// English advice, present unless `--lang tc`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advice: Option<AdviceRow>,
    /// Traditional Chinese advice, present for `--lang tc` and `--lang both`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advice_zh: Option<AdviceRow>,
}

/// The `advice --json` payload.
#[derive(Debug, Clone, Serialize)]
pub struct AdviceReport {
    /// Every requested band, low to serious.
    pub bands: Vec<AdviceBand>,
    /// Language selector that produced this report.
    pub lang: &'static str,
    /// Where the advice came from.
    pub source: String,
    /// When this process assembled the report, in Hong Kong time.
    pub fetched_at: String,
}

/// Band table: risk label, AQHI range and the four English rows, in the order
/// EPD publishes them (low to serious).
const BANDS: [(&str, &str, [&str; 4]); 5] = [
    (
        "Low",
        "1-3",
        [
            "No response action is required.",
            "No response action is required.",
            "No response action is required.",
            "No response action is required.",
        ],
    ),
    (
        "Moderate",
        "4-6",
        [
            "No response action is normally required. Individuals who are experiencing symptoms are advised to consider reducing outdoor physical exertion.",
            "No response action is required.",
            "No response action is required.",
            "No response action is required.",
        ],
    ),
    (
        "High",
        "7",
        [
            "People with existing heart or respiratory illnesses are advised to reduce outdoor physical exertion, and to reduce the time of their stay outdoors, especially in areas with heavy traffic. They should also seek advice from a medical doctor before participating in sport activities and take more breaks during physical activities.",
            "Children and the elderly are advised to reduce outdoor physical exertion, and to reduce the time of their stay outdoors, especially in areas with heavy traffic.",
            "No response action is required.",
            "No response action is required.",
        ],
    ),
    (
        "Very High",
        "8-10",
        [
            "People with existing heart or respiratory illnesses are advised to reduce to the minimum outdoor physical exertion, and to reduce to the minimum the time of their stay outdoors, especially in areas with heavy traffic.",
            "Children and the elderly are advised to reduce to the minimum outdoor physical exertion, and to reduce to the minimum the time of their stay outdoors, especially in areas with heavy traffic.",
            "Employers of outdoor workers performing heavy manual work are advised to assess the risk of outdoor work, and take appropriate preventive measures to protect the health of their employees such as reducing outdoor physical exertion, and reducing the time of their stay outdoors, especially in areas with heavy traffic.",
            "The general public is advised to reduce outdoor physical exertion, and to reduce the time of their stay outdoors, especially in areas with heavy traffic.",
        ],
    ),
    (
        "Serious",
        "10+",
        [
            "People with existing heart or respiratory illnesses are advised to avoid outdoor physical exertion, and to avoid staying outdoors, especially in areas with heavy traffic.",
            "Children and the elderly are advised to avoid outdoor physical exertion, and to avoid staying outdoors, especially in areas with heavy traffic.",
            "Employers of all outdoor workers are advised to assess the risk of outdoor work, and take appropriate preventive measures to protect the health of their employees such as reducing outdoor physical exertion, and reducing the time of their stay outdoors, especially in areas with heavy traffic.",
            "The general public is advised to reduce to the minimum outdoor physical exertion, and to reduce to the minimum the time of their stay outdoors, especially in areas with heavy traffic.",
        ],
    ),
];

/// Number of bands EPD publishes.
pub const BAND_COUNT: usize = BANDS.len();

/// Band index (0-4) for an AQHI value; values above 11 clamp to `Serious`.
#[must_use]
pub fn band_index_for_aqhi(aqhi: i64) -> usize {
    match aqhi {
        i64::MIN..=3 => 0,
        4..=6 => 1,
        7 => 2,
        8..=10 => 3,
        _ => 4,
    }
}

/// Risk label and AQHI range for a band index.
#[must_use]
pub fn band_label(index: usize) -> (&'static str, &'static str) {
    let (risk, range, _) = BANDS[index.min(BAND_COUNT - 1)];
    (risk, range)
}

/// The compiled-in English advice for one band.
#[must_use]
pub fn english_row(index: usize) -> AdviceRow {
    let (_, _, rows) = BANDS[index.min(BAND_COUNT - 1)];
    AdviceRow {
        heart_and_respiratory: rows[0].to_string(),
        children_and_elderly: rows[1].to_string(),
        outdoor_workers: rows[2].to_string(),
        general_public: rows[3].to_string(),
    }
}

/// Parse a `--risk` value into a band index.
///
/// Accepts the risk labels (`low`, `moderate`, `high`, `very-high`, `serious`,
/// also with spaces or underscores) and EPD's AQHI ranges (`1-3`, `4-6`, `7`,
/// `8-10`, `10+`, `11`).
///
/// # Errors
///
/// Returns [`Error::Usage`] for anything else.
pub fn parse_risk(value: &str) -> Result<usize> {
    let normalised = value.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    let index = match normalised.as_str() {
        "low" | "1-3" => 0,
        "moderate" | "4-6" => 1,
        "high" | "7" => 2,
        "very-high" | "8-10" => 3,
        "serious" | "10+" | "11" => 4,
        other => {
            return Err(Error::usage(format!(
                "--risk must be low, moderate, high, very-high or serious (or 1-3, 4-6, 7, 8-10, 11), got {other:?}"
            )));
        }
    };
    Ok(index)
}

/// Build an advice report.
///
/// English advice needs no network. `tc` and `both` fetch the app API; if that
/// fetch fails the English rows are still returned, with a note in the source
/// label, so an outage cannot hide the official advice.
///
/// # Errors
///
/// Returns [`Error::Http`] only when Chinese was the sole requested language and
/// the request failed.
pub fn fetch(lang: Lang, risk: Option<usize>) -> Result<AdviceReport> {
    let chinese = if lang.wants_chinese() {
        match fetch_chinese() {
            Ok(rows) => rows,
            Err(error) => {
                if !lang.wants_english() {
                    return Err(error);
                }
                eprintln!(
                    "hkpollution: warning: could not fetch Traditional Chinese advice ({error}); returning the English table only"
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    let indices: Vec<usize> = match risk {
        Some(index) => vec![index],
        None => (0..BAND_COUNT).collect(),
    };

    let bands: Vec<AdviceBand> = indices
        .into_iter()
        .map(|index| {
            let (risk_label, range) = band_label(index);
            let chinese_row = chinese
                .iter()
                .find(|(entry_index, _)| *entry_index == index)
                .map(|(_, row)| row.clone());
            AdviceBand {
                risk: risk_label,
                aqhi_range: range,
                advice: if lang.wants_english() {
                    Some(english_row(index))
                } else {
                    None
                },
                advice_zh: chinese_row,
            }
        })
        .collect();

    let source = if lang.wants_chinese() && !chinese.is_empty() {
        SOURCE_ZH
    } else {
        SOURCE_EN
    };

    Ok(AdviceReport {
        bands,
        lang: lang.label(),
        source: source.to_string(),
        fetched_at: now_hkt_iso8601(),
    })
}

fn fetch_chinese() -> Result<Vec<(usize, AdviceRow)>> {
    let body = http::get_cached(HEALTH_ADVICES_URL, CACHE_TTL_SECS)?;
    parse::parse_health_advices_json(&body)
}

#[cfg(test)]
mod tests {
    use super::{
        BAND_COUNT, Lang, band_index_for_aqhi, band_label, english_row, fetch, parse_risk,
    };

    #[test]
    fn maps_aqhi_values_to_bands() {
        assert_eq!(band_index_for_aqhi(1), 0);
        assert_eq!(band_index_for_aqhi(3), 0);
        assert_eq!(band_index_for_aqhi(4), 1);
        assert_eq!(band_index_for_aqhi(7), 2);
        assert_eq!(band_index_for_aqhi(8), 3);
        assert_eq!(band_index_for_aqhi(10), 3);
        assert_eq!(band_index_for_aqhi(11), 4);
        assert_eq!(band_index_for_aqhi(99), 4);
    }

    #[test]
    fn label_and_row_match_the_band_order() {
        assert_eq!(band_label(0), ("Low", "1-3"));
        assert_eq!(band_label(1), ("Moderate", "4-6"));
        assert_eq!(band_label(4), ("Serious", "10+"));
        assert_eq!(
            english_row(1).general_public,
            "No response action is required."
        );
        assert!(
            english_row(4)
                .outdoor_workers
                .starts_with("Employers of all")
        );
        for index in 0..BAND_COUNT {
            assert!(!english_row(index).heart_and_respiratory.is_empty());
        }
    }

    #[test]
    fn parses_risk_aliases() {
        assert_eq!(parse_risk("moderate").unwrap(), 1);
        assert_eq!(parse_risk("Very High").unwrap(), 3);
        assert_eq!(parse_risk("very_high").unwrap(), 3);
        assert_eq!(parse_risk("8-10").unwrap(), 3);
        assert_eq!(parse_risk("11").unwrap(), 4);
        assert!(parse_risk("apocalyptic").is_err());
    }

    #[test]
    fn english_needs_no_network() {
        let report = fetch(Lang::En, None).unwrap();
        assert_eq!(report.bands.len(), BAND_COUNT);
        assert_eq!(report.source, "EPD");
        assert!(report.bands.iter().all(|band| band.advice.is_some()));
        assert!(report.bands.iter().all(|band| band.advice_zh.is_none()));

        let one = fetch(Lang::En, Some(1)).unwrap();
        assert_eq!(one.bands.len(), 1);
        assert_eq!(one.bands[0].risk, "Moderate");
    }

    #[test]
    fn parses_lang_values() {
        assert_eq!(Lang::parse("EN").unwrap(), Lang::En);
        assert_eq!(Lang::parse("tc").unwrap(), Lang::Tc);
        assert_eq!(Lang::parse("both").unwrap(), Lang::Both);
        assert!(Lang::parse("fr").is_err());
    }
}
