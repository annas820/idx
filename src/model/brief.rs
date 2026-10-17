//! `stocks brief`: everything idx knows about one ticker in a single call.
//! Each section is the same object the dedicated command returns (or a
//! trimmed version of it) so an agent learns one shape per topic; a section
//! is `null` when its source failed and the reason sits in `warnings`.

use chrono::{DateTime, NaiveDate, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{
    DividendEvent, Fundamentals, Holding, NewsItem, Performance, Quote, SentimentLabel, Symbol,
    Technical,
};

/// Dividend facts without the full event list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DividendSummary {
    pub ttm_total: Option<f64>,
    pub ttm_payments: usize,
    pub ttm_yield_pct: Option<f64>,
    pub consecutive_years: usize,
    pub last: Option<DividendEvent>,
    pub payments_on_record: usize,
}

/// The local KSEI register, reduced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OwnershipSummary {
    pub as_of: NaiveDate,
    /// Sum over listed holders (>1 % each).
    pub listed_pct: f64,
    /// Part of `listed_pct` held by holders flagged foreign (`A`).
    pub foreign_pct: f64,
    pub holders_listed: usize,
    /// Largest five.
    pub top_holders: Vec<Holding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NewsSummary {
    /// Headlines that mention the company in the current feeds.
    pub company_matches: usize,
    pub sentiment: SentimentLabel,
    /// Mean lexicon score over those headlines, -1..+1.
    pub score: f64,
    /// Up to five company headlines, newest first.
    pub headlines: Vec<NewsItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Brief {
    pub symbol: Symbol,
    pub name: Option<String>,
    /// When the brief was assembled (UTC).
    pub generated_at: DateTime<Utc>,
    /// Plain-language key facts, one per line, built from the sections below.
    pub key_points: Vec<String>,
    pub quote: Option<Quote>,
    /// Same as `stocks fundamental`, verdict included.
    pub fundamentals: Option<Fundamentals>,
    /// Same as `stocks technical` over one year.
    pub technical: Option<Technical>,
    /// Same as `stocks returns` with the IHSG benchmark.
    pub returns: Option<Performance>,
    pub dividends: Option<DividendSummary>,
    pub ownership: Option<OwnershipSummary>,
    pub news: Option<NewsSummary>,
    /// Sections that are `null` because their source failed; details in `warnings`.
    pub sections_failed: Vec<String>,
}
