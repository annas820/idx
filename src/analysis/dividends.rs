//! Totals, trailing yield and streak over dividend events. Pure.

use std::collections::BTreeMap;

use chrono::{Datelike, Duration, NaiveDate};

use crate::model::{DividendHistory, DividendYear};

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Fills the computed fields of `h` as seen on `today`.
pub fn enrich(h: &mut DividendHistory, today: NaiveDate) {
    h.dividends.sort_by_key(|d| d.date);
    h.splits.sort_by_key(|s| s.date);
    h.as_of = today;
    h.last = h.dividends.last().cloned();

    let mut by_year: BTreeMap<i32, (usize, f64)> = BTreeMap::new();
    for d in &h.dividends {
        let e = by_year.entry(d.date.year()).or_default();
        e.0 += 1;
        e.1 += d.amount;
    }
    let mut prev: Option<(i32, f64)> = None;
    h.years = by_year
        .into_iter()
        .map(|(year, (payments, total))| {
            let change_pct = match prev {
                Some((py, pt)) if py + 1 == year && pt > 0.0 => {
                    Some(round2((total - pt) / pt * 100.0))
                }
                _ => None,
            };
            prev = Some((year, total));
            DividendYear {
                year,
                payments,
                total: round2(total),
                change_pct,
            }
        })
        .collect();

    let cutoff = today - Duration::days(365);
    let ttm: Vec<f64> = h
        .dividends
        .iter()
        .filter(|d| d.date > cutoff && d.date <= today)
        .map(|d| d.amount)
        .collect();
    h.ttm_payments = ttm.len();
    h.ttm_total = if ttm.is_empty() {
        None
    } else {
        Some(round2(ttm.iter().sum()))
    };
    h.ttm_yield_pct = match (h.ttm_total, h.price) {
        (Some(t), Some(p)) if p > 0.0 => Some(round2(t / p * 100.0)),
        _ => None,
    };

    h.consecutive_years = 0;
    if let Some(latest) = h.years.last().map(|y| y.year) {
        let mut year = latest;
        for y in h.years.iter().rev() {
            if y.year != year {
                break;
            }
            h.consecutive_years += 1;
            year -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DividendEvent, Symbol};

    fn d(y: i32, m: u32, day: u32, amount: f64) -> DividendEvent {
        DividendEvent {
            date: NaiveDate::from_ymd_opt(y, m, day).unwrap(),
            amount,
        }
    }

    #[test]
    fn totals_yield_and_streak() {
        let mut h = DividendHistory::raw(
            Symbol::parse("X").unwrap(),
            None,
            None,
            Some(1000.0),
            None,
            vec![
                d(2026, 3, 1, 30.0),
                d(2025, 12, 1, 10.0),
                d(2025, 3, 1, 25.0),
                d(2023, 3, 1, 20.0), // 2024 skipped: streak is 2 (2025, 2026)
            ],
            vec![],
        );
        enrich(&mut h, NaiveDate::from_ymd_opt(2026, 10, 17).unwrap());
        assert_eq!(h.dividends[0].date.year(), 2023, "sorted oldest first");
        assert_eq!(h.last.as_ref().unwrap().amount, 30.0);
        assert_eq!(h.ttm_payments, 2);
        assert_eq!(h.ttm_total, Some(40.0));
        assert_eq!(h.ttm_yield_pct, Some(4.0));
        assert_eq!(h.consecutive_years, 2);
        let years: Vec<(i32, usize, f64, Option<f64>)> = h
            .years
            .iter()
            .map(|y| (y.year, y.payments, y.total, y.change_pct))
            .collect();
        assert_eq!(
            years,
            [
                (2023, 1, 20.0, None),
                (2025, 2, 35.0, None), // gap year before it: no comparison
                (2026, 1, 30.0, Some(-14.29)),
            ]
        );
        // One payment 2026-03 then nothing: still counts 2026 and 2025.
        let mut none = DividendHistory::raw(
            Symbol::parse("Y").unwrap(),
            None,
            None,
            None,
            None,
            vec![],
            vec![],
        );
        enrich(&mut none, NaiveDate::from_ymd_opt(2026, 10, 17).unwrap());
        assert!(none.last.is_none() && none.ttm_total.is_none());
        assert_eq!(none.consecutive_years, 0);
    }
}
