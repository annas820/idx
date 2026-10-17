//! `stocks returns`: price performance computed locally from daily candles.
//! Every number here is derived by idx from the provider's close prices
//! (`analysis::returns`); nothing is taken from a source's own statistics.

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Symbol;

/// Total price return over standard windows, percent. A window is `null`
/// when the candles do not reach back far enough.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReturnWindows {
    #[serde(rename = "1w")]
    pub w1: Option<f64>,
    #[serde(rename = "1m")]
    pub m1: Option<f64>,
    #[serde(rename = "3m")]
    pub m3: Option<f64>,
    #[serde(rename = "6m")]
    pub m6: Option<f64>,
    /// Since the last close of the previous calendar year.
    pub ytd: Option<f64>,
    #[serde(rename = "1y")]
    pub y1: Option<f64>,
    #[serde(rename = "3y")]
    pub y3: Option<f64>,
    #[serde(rename = "5y")]
    pub y5: Option<f64>,
}

impl ReturnWindows {
    pub const LABELS: [&'static str; 8] = ["1w", "1m", "3m", "6m", "ytd", "1y", "3y", "5y"];

    pub fn values(&self) -> [Option<f64>; 8] {
        [
            self.w1, self.m1, self.m3, self.m6, self.ytd, self.y1, self.y3, self.y5,
        ]
    }

    /// `self - other`, window by window; `null` where either side is.
    pub fn minus(&self, other: &ReturnWindows) -> ReturnWindows {
        let d = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (Some(a), Some(b)) => Some(((a - b) * 100.0).round() / 100.0),
            _ => None,
        };
        ReturnWindows {
            w1: d(self.w1, other.w1),
            m1: d(self.m1, other.m1),
            m3: d(self.m3, other.m3),
            m6: d(self.m6, other.m6),
            ytd: d(self.ytd, other.ytd),
            y1: d(self.y1, other.y1),
            y3: d(self.y3, other.y3),
            y5: d(self.y5, other.y5),
        }
    }
}

/// Largest peak-to-trough fall in close prices inside a window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Drawdown {
    /// Negative percent, e.g. `-34.2`.
    pub pct: f64,
    pub peak_date: NaiveDate,
    pub trough_date: NaiveDate,
    /// `true` when a later close got back to the peak.
    pub recovered: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DayMove {
    pub date: NaiveDate,
    /// Close-to-close change, percent.
    pub pct: f64,
}

/// The benchmark's own returns and the symbol's excess over them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BenchmarkReturns {
    /// E.g. `^JKSE` (IHSG).
    pub symbol: Symbol,
    pub name: Option<String>,
    pub returns: ReturnWindows,
    /// Symbol return minus benchmark return, percentage points per window.
    pub excess: ReturnWindows,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Performance {
    pub symbol: Symbol,
    pub name: Option<String>,
    pub currency: Option<String>,
    /// Date of the last candle used.
    pub as_of: NaiveDate,
    /// Last close.
    pub price: f64,
    /// Date of the first candle available.
    pub start: NaiveDate,
    pub candles_used: usize,
    pub returns: ReturnWindows,
    /// Compound annual growth rate over the 3y / 5y windows, percent.
    pub cagr_3y_pct: Option<f64>,
    pub cagr_5y_pct: Option<f64>,
    /// Annualised standard deviation of daily log returns over the last year, percent.
    pub volatility_1y_pct: Option<f64>,
    pub max_drawdown_1y: Option<Drawdown>,
    /// Over every candle in the window fetched (up to 5 years).
    pub max_drawdown_all: Option<Drawdown>,
    pub best_day_1y: Option<DayMove>,
    pub worst_day_1y: Option<DayMove>,
    pub high_52w: Option<f64>,
    pub low_52w: Option<f64>,
    /// `(price / high_52w - 1) * 100`, zero or negative.
    pub pct_from_52w_high: Option<f64>,
    pub pct_from_52w_low: Option<f64>,
    pub days_since_52w_high: Option<i64>,
    /// `null` when the benchmark was disabled or could not be fetched.
    pub benchmark: Option<BenchmarkReturns>,
}
