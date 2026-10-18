//! Exchange-wide data: trading holidays, instruments, indices, the
//! corporate-action calendar, foreign flow and broker activity. Shared by
//! the CLI and the MCP server like the rest of `service`.

use chrono::{Datelike, NaiveDate};

use crate::error::{IdxError, Result};
use crate::holidays;
use crate::model::Holidays;
use crate::providers::Sourced;

/// Provider name for data that ships inside the binary.
pub const BUNDLED: &str = "bundled";

/// The trading-holiday list for `year` (every listed year when `None`),
/// limited to dates from `today` on when `upcoming`.
pub fn holidays(year: Option<i32>, upcoming: bool, today: NaiveDate) -> Result<Sourced<Holidays>> {
    let covered = holidays::years();
    let mut warnings = Vec::new();
    if let Some(y) = year {
        if !(1990..=2100).contains(&y) {
            return Err(IdxError::Usage(format!("year {y} is out of range")));
        }
        if !holidays::covers(y) {
            warnings.push(format!(
                "no holiday list for {y}; covered: {}",
                covered
                    .iter()
                    .map(|y| y.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    let list = holidays::all()
        .into_iter()
        .filter(|h| year.is_none_or(|y| h.date.year() == y))
        .filter(|h| !upcoming || h.date >= today)
        .collect();
    let mut s = Sourced::fresh(
        Holidays {
            year,
            trading_days: year
                .filter(|y| holidays::covers(*y))
                .map(holidays::trading_days),
            years_covered: covered,
            today,
            today_is_trading_day: holidays::is_trading_day(today),
            next_trading_day: holidays::next_trading_day(today),
            holidays: list,
        },
        BUNDLED,
    );
    s.warnings = warnings;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn holidays_for_a_year_with_trading_facts() {
        let s = holidays(Some(2026), false, d(2026, 10, 18)).unwrap();
        assert_eq!(s.provider, "bundled");
        assert_eq!(s.data.holidays.len(), 22);
        assert_eq!(s.data.trading_days, Some(239));
        assert!(!s.data.today_is_trading_day, "Sunday");
        assert_eq!(s.data.next_trading_day, d(2026, 10, 19));
        assert!(s.warnings.is_empty());

        let up = holidays(Some(2026), true, d(2026, 10, 18)).unwrap();
        let dates: Vec<_> = up.data.holidays.iter().map(|h| h.date).collect();
        assert_eq!(dates, [d(2026, 12, 24), d(2026, 12, 25), d(2026, 12, 31)]);
    }

    #[test]
    fn uncovered_year_warns_and_has_no_count() {
        let s = holidays(Some(2030), false, d(2026, 10, 18)).unwrap();
        assert!(s.data.holidays.is_empty());
        assert_eq!(s.data.trading_days, None);
        assert!(s.warnings[0].contains("no holiday list for 2030"));
        assert!(holidays(Some(10_000), false, d(2026, 10, 18)).is_err());
        let all = holidays(None, false, d(2026, 10, 18)).unwrap();
        assert_eq!(all.data.year, None);
        assert_eq!(all.data.trading_days, None);
        assert!(all.data.holidays.len() > 30);
    }
}
