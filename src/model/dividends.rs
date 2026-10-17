//! `stocks dividends`: cash dividend and split events for one ticker.
//!
//! Events come from the provider as reported (Yahoo returns split-adjusted
//! amounts keyed by ex-date). The totals, trailing-twelve-month yield and the
//! streak are computed locally by `analysis::dividends` from those events and
//! say so in their doc comments.

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Symbol;

/// One cash dividend.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DividendEvent {
    /// Ex-dividend date as given by the source (WIB calendar date).
    pub date: NaiveDate,
    /// Per-share amount in `currency`, as returned by the source
    /// (split-adjusted on Yahoo).
    pub amount: f64,
}

/// One stock split.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SplitEvent {
    pub date: NaiveDate,
    pub numerator: f64,
    pub denominator: f64,
    /// Source label such as `5:1`.
    pub ratio: String,
}

/// Dividends paid in one calendar year (ex-date basis). Computed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DividendYear {
    pub year: i32,
    pub payments: usize,
    pub total: f64,
    /// Change of `total` versus the previous listed year, percent; `null` for the first.
    pub change_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DividendHistory {
    pub symbol: Symbol,
    pub name: Option<String>,
    pub currency: Option<String>,
    /// Last price from the same response, used for the yield.
    pub price: Option<f64>,
    /// Date of that price.
    pub price_date: Option<NaiveDate>,
    /// Oldest first.
    pub dividends: Vec<DividendEvent>,
    /// Oldest first.
    pub splits: Vec<SplitEvent>,
    /// Oldest first. Computed from `dividends`.
    pub years: Vec<DividendYear>,
    /// Most recent payment; `null` when there is none.
    pub last: Option<DividendEvent>,
    /// Sum of payments with an ex-date in the last 365 days. Computed.
    pub ttm_total: Option<f64>,
    pub ttm_payments: usize,
    /// `ttm_total / price * 100`. Computed by idx, not reported by the source.
    pub ttm_yield_pct: Option<f64>,
    /// Calendar years with at least one payment, counted back from the most
    /// recent paying year without a gap. Computed.
    pub consecutive_years: usize,
    /// Date the computed fields were evaluated against.
    pub as_of: NaiveDate,
}

impl DividendHistory {
    /// Raw shell before `analysis::dividends::enrich` fills the computed fields.
    pub fn raw(
        symbol: Symbol,
        name: Option<String>,
        currency: Option<String>,
        price: Option<f64>,
        price_date: Option<NaiveDate>,
        dividends: Vec<DividendEvent>,
        splits: Vec<SplitEvent>,
    ) -> Self {
        DividendHistory {
            symbol,
            name,
            currency,
            price,
            price_date,
            dividends,
            splits,
            years: vec![],
            last: None,
            ttm_total: None,
            ttm_payments: 0,
            ttm_yield_pct: None,
            consecutive_years: 0,
            as_of: NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch"),
        }
    }
}
