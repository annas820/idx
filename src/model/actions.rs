//! `idx stocks actions`: one company's corporate actions, shareholders'
//! meetings (RUPS) and listed warrants and rights, as the source publishes
//! them. Dates are WIB calendar dates; `null` where the source shows none.

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Symbol;

/// What kind of corporate action a row is, normalised by idx from the
/// source's label (which is kept in `label`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    CashDividend,
    StockDividend,
    StockSplit,
    ReverseSplit,
    Bonus,
    RightsIssue,
    Warrant,
    Other,
}

impl ActionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionKind::CashDividend => "cash_dividend",
            ActionKind::StockDividend => "stock_dividend",
            ActionKind::StockSplit => "stock_split",
            ActionKind::ReverseSplit => "reverse_split",
            ActionKind::Bonus => "bonus",
            ActionKind::RightsIssue => "rights_issue",
            ActionKind::Warrant => "warrant",
            ActionKind::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CorporateAction {
    pub kind: ActionKind,
    /// The source's label, e.g. `Cash Deviden` (sic), `Stock Split`, `Right Issue`.
    pub label: String,
    /// IDR per share: the cash dividend, or a rights issue's exercise price;
    /// `null` when the source shows 0 (splits, bonus shares).
    pub amount: Option<f64>,
    /// The ratio as published, old : new. A 1:5 split turns one share into
    /// five; 2:1 rights give one right per two shares; 1:1 bonus gives one
    /// new share per share. `null` for cash dividends.
    pub ratio_old: Option<f64>,
    pub ratio_new: Option<f64>,
    /// Last day to trade with the entitlement (regular market).
    pub cum_date: Option<NaiveDate>,
    /// First day without it.
    pub ex_date: Option<NaiveDate>,
    /// Recording date (DPS).
    pub recording_date: Option<NaiveDate>,
    /// Payment or distribution date.
    pub distribution_date: Option<NaiveDate>,
    /// The source's "right end" date: the end of a rights trading period,
    /// or the last step of a split or bonus; `null` for cash dividends.
    pub end_date: Option<NaiveDate>,
    /// As published: `Final`, `Tentative`, `Pending`, ...
    pub status: Option<String>,
    /// The announcement text, tags stripped.
    pub details: Option<String>,
}

/// One shareholders' meeting (RUPS).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShareholderMeeting {
    pub date: Option<NaiveDate>,
    /// Local time as published, `HH:MM:SS` (WIB).
    pub time: Option<String>,
    pub status: Option<String>,
    pub venue: Option<String>,
    /// Notice text with the agenda, tags stripped.
    pub agenda: Option<String>,
}

/// A listed warrant or right (e.g. `ENRG-R`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WarrantRight {
    /// Code of the listed instrument.
    pub code: String,
    /// `Right` or `Warrant`, as published.
    pub kind: String,
    /// IDR per share.
    pub exercise_price: Option<f64>,
    /// Instruments issued.
    pub shares: Option<f64>,
    pub listing_date: Option<NaiveDate>,
    pub maturity_date: Option<NaiveDate>,
    pub status: Option<String>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CorporateActions {
    pub symbol: Symbol,
    /// Company name from the bundled universe; `null` when unknown.
    pub name: Option<String>,
    /// Events from this date on (the source's filter), upcoming ones included.
    pub since: NaiveDate,
    /// Newest cum date first.
    pub actions: Vec<CorporateAction>,
    /// Newest first.
    pub meetings: Vec<ShareholderMeeting>,
    /// Newest listing first.
    pub warrants_rights: Vec<WarrantRight>,
}
