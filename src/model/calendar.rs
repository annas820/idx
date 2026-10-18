//! `idx market holidays`: the exchange's trading-holiday list.

use chrono::NaiveDate;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One weekday the exchange is closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Holiday {
    pub date: NaiveDate,
    /// English weekday name, e.g. `Monday`.
    pub weekday: String,
    /// Name as IDX publishes it in Indonesian.
    pub name: String,
    /// Name from IDX's English calendar; `null` when the list has none.
    pub name_en: Option<String>,
}

/// Holidays for one year (or every listed year) plus the trading-day facts
/// derived from them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Holidays {
    /// The year asked for; `null` when every listed year is shown.
    pub year: Option<i32>,
    /// Years the holiday list covers.
    pub years_covered: Vec<i32>,
    /// Weekdays minus holidays in `year`; `null` when no single year was
    /// asked for or the list does not cover it.
    pub trading_days: Option<u32>,
    /// Today in Jakarta.
    pub today: NaiveDate,
    /// `false` on weekends and listed holidays (session hours are not checked).
    pub today_is_trading_day: bool,
    /// The first trading day after today.
    pub next_trading_day: NaiveDate,
    /// Oldest first.
    pub holidays: Vec<Holiday>,
}
