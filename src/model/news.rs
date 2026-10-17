//! `stocks news` and `stocks sentiment` output.

use chrono::{DateTime, FixedOffset};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Symbol;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NewsItem {
    pub title: String,
    pub url: String,
    /// Publisher feed, e.g. `kontan/investasi`, `cnbcindonesia/market`.
    pub source: String,
    /// Publication time as given by the feed, in WIB.
    pub published: Option<DateTime<FixedOffset>>,
    /// Feed summary with HTML stripped; may be empty.
    pub summary: String,
    /// How the item matched the ticker: `ticker`, `name` or `all` (no filter).
    pub matched_by: String,
    /// `company` when the item mentions the requested ticker, else `market`.
    /// Company items always come first in `items`.
    pub scope: String,
    /// Lexicon sentiment of title + summary: -1 (all negative) to +1 (all positive).
    pub sentiment: f64,
    pub positive_terms: Vec<String>,
    pub negative_terms: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NewsReport {
    /// `null` for the general market feed.
    pub symbol: Option<Symbol>,
    /// Company name used for matching, when a symbol was given.
    pub matched_name: Option<String>,
    pub sources: Vec<String>,
    /// Items fetched across all feeds before filtering.
    pub scanned: usize,
    /// Headlines that mention the company (before `limit`); 0 without a symbol.
    pub company_matches: usize,
    /// `true` when market headlines were left out (`--company-only`).
    pub company_only: bool,
    pub items: Vec<NewsItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SentimentLabel {
    Positive,
    Negative,
    Neutral,
    NoCoverage,
}

impl SentimentLabel {
    pub fn as_str(&self) -> &'static str {
        match self {
            SentimentLabel::Positive => "positive",
            SentimentLabel::Negative => "negative",
            SentimentLabel::Neutral => "neutral",
            SentimentLabel::NoCoverage => "no coverage",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SentimentReport {
    pub symbol: Symbol,
    pub matched_name: Option<String>,
    pub label: SentimentLabel,
    /// Mean item sentiment, -1 to +1.
    pub score: f64,
    pub items: usize,
    pub positive: usize,
    pub negative: usize,
    pub neutral: usize,
    /// Period covered by the matched items.
    pub oldest: Option<DateTime<FixedOffset>>,
    pub newest: Option<DateTime<FixedOffset>>,
    /// Most frequent sentiment terms with their counts.
    pub top_positive_terms: Vec<(String, usize)>,
    pub top_negative_terms: Vec<(String, usize)>,
    /// The headlines that drove the score, strongest first.
    pub headlines: Vec<NewsItem>,
    pub method: String,
}
