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

/// What an exchange-calendar entry is about. Classified by idx from the
/// entry's type and wording; `other` when no rule matched.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, clap::ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum CalendarCategory {
    /// Cash or stock dividend dates (cum, ex, recording, payment).
    Dividend,
    /// Rights issue (HMETD) dates.
    Rights,
    /// Bonus shares.
    Bonus,
    /// Stock split or reverse split.
    Split,
    /// Warrants.
    Warrant,
    /// Shareholders' meetings (RUPS): plan, notice, result.
    Rups,
    /// Public expose: plan, material, result.
    #[value(alias = "expose", alias = "public_expose")]
    PublicExpose,
    /// Share buyback reports.
    Buyback,
    /// Share listings.
    Listing,
    /// Bond and sukuk events: bondholder meetings, coupons, maturities, listings.
    Bond,
    Other,
}

impl CalendarCategory {
    pub const ALL: [CalendarCategory; 11] = [
        CalendarCategory::Dividend,
        CalendarCategory::Rights,
        CalendarCategory::Bonus,
        CalendarCategory::Split,
        CalendarCategory::Warrant,
        CalendarCategory::Rups,
        CalendarCategory::PublicExpose,
        CalendarCategory::Buyback,
        CalendarCategory::Listing,
        CalendarCategory::Bond,
        CalendarCategory::Other,
    ];

    /// Shown when no category is asked for: everything but bond events and
    /// the daily buyback reports.
    pub const DEFAULT: [CalendarCategory; 9] = [
        CalendarCategory::Dividend,
        CalendarCategory::Rights,
        CalendarCategory::Bonus,
        CalendarCategory::Split,
        CalendarCategory::Warrant,
        CalendarCategory::Rups,
        CalendarCategory::PublicExpose,
        CalendarCategory::Listing,
        CalendarCategory::Other,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            CalendarCategory::Dividend => "dividend",
            CalendarCategory::Rights => "rights",
            CalendarCategory::Bonus => "bonus",
            CalendarCategory::Split => "split",
            CalendarCategory::Warrant => "warrant",
            CalendarCategory::Rups => "rups",
            CalendarCategory::PublicExpose => "public_expose",
            CalendarCategory::Buyback => "buyback",
            CalendarCategory::Listing => "listing",
            CalendarCategory::Bond => "bond",
            CalendarCategory::Other => "other",
        }
    }

    /// `dividend,rups`, `all` or `default` into categories.
    pub fn parse_list(raw: &str) -> crate::error::Result<Vec<CalendarCategory>> {
        let mut out = Vec::new();
        for piece in raw.split(|c: char| c == ',' || c.is_whitespace()) {
            let p = piece.trim();
            if p.is_empty() {
                continue;
            }
            let add: Vec<CalendarCategory> = match p.to_ascii_lowercase().as_str() {
                "all" => Self::ALL.to_vec(),
                "default" => Self::DEFAULT.to_vec(),
                _ => vec![
                    <CalendarCategory as clap::ValueEnum>::from_str(p, true).map_err(|_| {
                        crate::error::IdxError::Usage(format!(
                            "unknown calendar type {p:?}; use all, default or {}",
                            Self::ALL.map(|c| c.as_str()).join(", ")
                        ))
                    })?,
                ],
            };
            for c in add {
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        if out.is_empty() {
            out = Self::DEFAULT.to_vec();
        }
        Ok(out)
    }
}

/// One entry on the exchange calendar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CalendarEvent {
    /// Calendar date of the entry.
    pub date: NaiveDate,
    /// Issuer code as the source lists it: a ticker such as `BBCA`, or an
    /// issuer like `DJPU` (the government) for bonds.
    pub code: String,
    pub category: CalendarCategory,
    /// Which step the date marks, classified by idx from the wording:
    /// `cum`, `ex`, `recording`, `payment`, `distribution`, `listing`,
    /// `trading_start`, `trading_end` for dividends and rights; `plan`,
    /// `notice`, `result`, `material` for meetings and exposes; `coupon`,
    /// `principal`, `maturity` for bonds; `report` for buybacks. `null` when
    /// no rule matched.
    pub milestone: Option<String>,
    /// Text as published.
    pub description: String,
    pub location: Option<String>,
    /// Meeting date and time (WIB) for shareholders' meetings; `null` when
    /// not given.
    pub meeting_at: Option<chrono::NaiveDateTime>,
    /// Public expose date; `null` when not given.
    pub expose_date: Option<NaiveDate>,
    /// The source's own type field (e.g. `Luar Biasa`, `tahunan`, `rupo`), as published.
    pub source_type: Option<String>,
    /// The source's step field (e.g. `rencana`, `pgl`), as published.
    pub source_step: Option<String>,
    /// The source's entry id.
    pub id: u64,
}

/// Calendar entries over a date range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Calendar {
    pub from: NaiveDate,
    pub to: NaiveDate,
    /// Issuer filter, if any.
    pub symbol: Option<String>,
    /// Categories shown.
    pub categories: Vec<CalendarCategory>,
    /// Entries in the range before the category filter.
    pub total: usize,
    /// Oldest first.
    pub events: Vec<CalendarEvent>,
}
