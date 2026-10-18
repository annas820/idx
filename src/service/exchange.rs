//! Exchange-wide data: trading holidays, instruments, indices, the
//! corporate-action calendar, foreign flow and broker activity. Shared by
//! the CLI and the MCP server like the rest of `service`.

use std::future::Future;
use std::sync::Arc;

use chrono::{Datelike, NaiveDate};

use crate::brokers;
use crate::error::{IdxError, Result};
use crate::holidays;
use crate::http::Fetcher;
use crate::model::{
    Calendar, CalendarCategory, CalendarEvent, Holidays, IndexBoard, Instrument, InstrumentList,
    Member, MemberList,
};
use crate::model::{CorporateActions, Symbol};
use crate::providers::idx::{self, CalendarRange};
use crate::providers::ipot;
use crate::providers::Sourced;
use crate::universe::{self, Issuer};

/// How far back `corporate_actions` reaches when no date is given.
pub const ACTIONS_DEFAULT_DAYS: u64 = 365;

/// One company's corporate actions, shareholders' meetings and listed
/// warrants and rights (IPOT) from `since` on (default: a year back),
/// upcoming events included.
pub async fn corporate_actions(
    fetcher: &Arc<Fetcher>,
    universe: &[Issuer],
    symbol: &Symbol,
    since: Option<NaiveDate>,
    today: NaiveDate,
) -> Result<Sourced<CorporateActions>> {
    if symbol.is_index() {
        return Err(IdxError::Usage(format!(
            "{symbol} is an index; corporate actions belong to stocks"
        )));
    }
    let since = since.unwrap_or(today - chrono::Days::new(ACTIONS_DEFAULT_DAYS));
    let mut s = ipot::corporate_actions(fetcher, symbol, since).await?;
    s.data.name = universe
        .iter()
        .find(|i| &i.symbol == symbol)
        .and_then(|i| i.name.clone());
    Ok(s)
}

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

/// Trading days `latest_session` walks back over before giving up.
const SESSION_LOOKBACK: usize = 10;

/// Why `date` had no session, for messages.
fn closed_reason(date: NaiveDate) -> String {
    if let Some(h) = holidays::holiday(date) {
        format!("{date} is a trading holiday ({})", h.name)
    } else if matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
        format!("{date} is a {}", date.format("%A"))
    } else {
        format!("nothing was published for {date}")
    }
}

/// The day's list from `fetch`: for `date` when given, else for the latest
/// trading day at or before `today` that has one. Weekends and listed
/// holidays are skipped without a request. An empty explicit date is
/// `NotFound`.
async fn session<T, F, Fut>(
    date: Option<NaiveDate>,
    today: NaiveDate,
    fetch: F,
) -> Result<(NaiveDate, Sourced<Vec<T>>)>
where
    F: Fn(NaiveDate) -> Fut,
    Fut: Future<Output = Result<Sourced<Vec<T>>>>,
{
    if let Some(d) = date {
        if d > today {
            return Err(IdxError::Usage(format!("{d} is in the future")));
        }
        let s = fetch(d).await?;
        if s.data.is_empty() {
            return Err(IdxError::NotFound(format!(
                "no trading data: {}",
                closed_reason(d)
            )));
        }
        return Ok((d, s));
    }
    let mut day = holidays::last_trading_day(today);
    for _ in 0..SESSION_LOOKBACK {
        let s = fetch(day).await?;
        if !s.data.is_empty() {
            return Ok((day, s));
        }
        day = holidays::last_trading_day(day.pred_opt().expect("date in range"));
    }
    Err(IdxError::NotFound(format!(
        "no session published in the {SESSION_LOOKBACK} trading days to {today}"
    )))
}

fn matches_search(search: Option<&str>, code: &str, name: Option<&str>) -> bool {
    let Some(q) = search.map(str::trim).filter(|q| !q.is_empty()) else {
        return true;
    };
    let q = q.to_lowercase();
    code.to_lowercase().contains(&q) || name.is_some_and(|n| n.to_lowercase().contains(&q))
}

/// Listed stocks from idx.co.id, filtered by board and by a code or name
/// substring. When idx.co.id cannot be reached and nothing is cached, the
/// bundled universe stands in (codes and names only) with a warning.
pub async fn instruments(
    fetcher: &Arc<Fetcher>,
    board: Option<&str>,
    search: Option<&str>,
) -> Result<Sourced<InstrumentList>> {
    let mut s = match idx::securities(fetcher).await {
        Ok(s) => s,
        Err(e) => {
            let list: Vec<Instrument> = universe::load(None)?
                .into_iter()
                .map(|i| Instrument {
                    symbol: i.symbol,
                    name: i.name,
                    listing_date: None,
                    shares: None,
                    board: None,
                })
                .collect();
            let mut s = Sourced::fresh(list, BUNDLED);
            s.warnings.push(format!(
                "{e}; listing the bundled universe instead (codes and names only, no listing dates, shares or boards)"
            ));
            s
        }
    };
    let board = board.map(str::trim).filter(|b| !b.is_empty());
    let total = s.data.len();
    let mut list: Vec<Instrument> = std::mem::take(&mut s.data)
        .into_iter()
        .filter(|i| {
            board.is_none_or(|b| {
                i.board
                    .as_deref()
                    .is_some_and(|x| x.to_lowercase().contains(&b.to_lowercase()))
            })
        })
        .filter(|i| matches_search(search, i.symbol.as_str(), i.name.as_deref()))
        .collect();
    list.sort_by(|a, b| a.symbol.cmp(&b.symbol));
    Ok(s.map(|_| InstrumentList {
        total,
        board: board.map(str::to_string),
        search: search
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_string),
        instruments: list,
    }))
}

/// Exchange members (broker codes) from idx.co.id, filtered by a code or
/// name substring; the bundled member list stands in when idx.co.id cannot
/// be reached and nothing is cached.
pub async fn members(fetcher: &Arc<Fetcher>, search: Option<&str>) -> Result<Sourced<MemberList>> {
    let mut s = match idx::members(fetcher).await {
        Ok(s) => s,
        Err(e) => {
            let list = brokers::all()
                .into_iter()
                .map(|(code, name)| Member {
                    code,
                    name: Some(name),
                    licenses: None,
                })
                .collect();
            let mut s = Sourced::fresh(list, BUNDLED);
            s.warnings.push(format!(
                "{e}; listing the bundled member list instead (no licences)"
            ));
            s
        }
    };
    let total = s.data.len();
    let mut list: Vec<Member> = std::mem::take(&mut s.data)
        .into_iter()
        .filter(|m| matches_search(search, &m.code, m.name.as_deref()))
        .collect();
    list.sort_by(|a, b| a.code.cmp(&b.code));
    Ok(s.map(|_| MemberList {
        total,
        search: search
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_string),
        members: list,
    }))
}

/// Every index on `date` (default: the latest session), optionally only
/// the given codes (`LQ45`, `IDX30`; `IHSG` and `^JKSE` mean `COMPOSITE`).
pub async fn indices(
    fetcher: &Arc<Fetcher>,
    date: Option<NaiveDate>,
    codes: &[String],
    today: NaiveDate,
) -> Result<Sourced<IndexBoard>> {
    let (day, mut s) = session(date, today, |d| idx::index_summary(fetcher, d, today)).await?;
    let wanted: Vec<String> = codes
        .iter()
        .flat_map(|c| c.split(|ch: char| ch == ',' || ch.is_whitespace()))
        .map(|c| c.trim().to_ascii_uppercase())
        .filter(|c| !c.is_empty())
        .map(|c| match c.as_str() {
            "IHSG" | "^JKSE" | "JKSE" => "COMPOSITE".to_string(),
            _ => c,
        })
        .collect();
    let mut rows = std::mem::take(&mut s.data);
    if !wanted.is_empty() {
        for w in &wanted {
            if !rows.iter().any(|r| &r.code == w) {
                s.warnings.push(format!("no index {w} on {day}"));
            }
        }
        rows.retain(|r| wanted.contains(&r.code));
        if rows.is_empty() {
            return Err(IdxError::NotFound(format!(
                "none of {} on {day}",
                wanted.join(", ")
            )));
        }
    }
    Ok(s.map(|_| IndexBoard {
        date: day,
        indices: rows,
    }))
}

/// Longest range `calendar` covers in one call.
pub const CALENDAR_MAX_DAYS: i64 = 92;
/// Days `calendar` looks ahead when no end is given.
pub const CALENDAR_DEFAULT_DAYS: u32 = 14;

/// Which part of the exchange calendar to show.
#[derive(Debug, Clone, Default)]
pub struct CalendarQuery {
    /// One day; wins over `from`/`to`.
    pub date: Option<NaiveDate>,
    /// Start of the range; default today.
    pub from: Option<NaiveDate>,
    /// End of the range; default `from` plus `days`.
    pub to: Option<NaiveDate>,
    /// Look-ahead when `to` is not given; default [`CALENDAR_DEFAULT_DAYS`].
    pub days: Option<u32>,
    /// One issuer code (`BBCA`).
    pub symbol: Option<String>,
    /// Categories to keep; empty means [`CalendarCategory::DEFAULT`].
    pub categories: Vec<CalendarCategory>,
}

/// First day of every month that `from..=to` touches.
fn months(from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let mut out = Vec::new();
    let mut m = from.with_day(1).expect("day 1 exists");
    while m <= to {
        out.push(m);
        m = m
            .checked_add_months(chrono::Months::new(1))
            .expect("date in range");
    }
    out
}

/// Exchange calendar entries (dividend and rights dates, shareholders'
/// meetings, public exposes, ...) over a day or a range, oldest first.
/// One request per day asked for, or per month the range touches.
pub async fn calendar(
    fetcher: &Arc<Fetcher>,
    q: &CalendarQuery,
    today: NaiveDate,
) -> Result<Sourced<Calendar>> {
    let (from, to) = match q.date {
        Some(d) => (d, d),
        None => {
            let from = q.from.unwrap_or(today);
            let days = q.days.unwrap_or(CALENDAR_DEFAULT_DAYS);
            let to =
                q.to.unwrap_or_else(|| from + chrono::Days::new(u64::from(days)));
            (from, to)
        }
    };
    if from > to {
        return Err(IdxError::Usage(format!(
            "the range starts ({from}) after it ends ({to})"
        )));
    }
    if (to - from).num_days() > CALENDAR_MAX_DAYS {
        return Err(IdxError::Usage(format!(
            "a calendar range covers at most {CALENDAR_MAX_DAYS} days"
        )));
    }
    let symbol = match q.symbol.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(crate::model::Symbol::parse(s)?.as_str().to_string()),
        None => None,
    };
    let ranges: Vec<CalendarRange> = if from == to {
        vec![CalendarRange::Day(from)]
    } else {
        months(from, to)
            .into_iter()
            .map(CalendarRange::Month)
            .collect()
    };
    let mut out: Option<Sourced<Vec<CalendarEvent>>> = None;
    for r in ranges {
        let s = idx::calendar(fetcher, r, symbol.as_deref()).await?;
        match &mut out {
            None => out = Some(s),
            Some(acc) => {
                acc.absorb(&s);
                acc.data.extend(s.data);
            }
        }
    }
    let mut s = out.expect("at least one range");
    let mut events: Vec<CalendarEvent> = std::mem::take(&mut s.data)
        .into_iter()
        .filter(|e| e.date >= from && e.date <= to)
        .collect();
    events.sort_by(|a, b| a.date.cmp(&b.date).then(a.id.cmp(&b.id)));
    // The source sometimes lists the same entry twice under two ids.
    let mut seen = std::collections::HashSet::new();
    events.retain(|e| seen.insert((e.date, e.code.clone(), e.description.clone())));
    let total = events.len();
    let categories = if q.categories.is_empty() {
        CalendarCategory::DEFAULT.to_vec()
    } else {
        q.categories.clone()
    };
    events.retain(|e| categories.contains(&e.category));
    Ok(s.map(|_| Calendar {
        from,
        to,
        symbol,
        categories,
        total,
        events,
    }))
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

    use crate::cache::Cache;
    use crate::http::{CacheMode, FixtureBackend};

    const SECURITIES: &str = include_str!("../../tests/fixtures/idx/securities_excerpt.json");
    const MEMBERS: &str = include_str!("../../tests/fixtures/idx/brokersearch.json");
    const INDICES: &str = include_str!("../../tests/fixtures/idx/indexsummary_20261016.json");
    const NO_TRADING: &str =
        include_str!("../../tests/fixtures/idx/stocksummary_20261018_no_trading.json");

    pub(super) fn fetcher(
        be: FixtureBackend,
    ) -> (Arc<Fetcher>, Arc<FixtureBackend>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let be = Arc::new(be);
        let f = Arc::new(Fetcher::new(
            be.clone(),
            Cache::new(dir.path()),
            CacheMode::Normal,
        ));
        (f, be, dir)
    }

    #[tokio::test]
    async fn instruments_filter_by_board_and_search() {
        let (f, _be, _d) =
            fetcher(FixtureBackend::new().route("GetSecuritiesStock", 200, SECURITIES));
        let s = instruments(&f, Some("akselerasi"), None).await.unwrap();
        assert_eq!(s.provider, "idx");
        assert!(s.data.total > s.data.instruments.len());
        assert!(s
            .data
            .instruments
            .iter()
            .all(|i| i.board.as_deref() == Some("Akselerasi")));
        let s = instruments(&f, None, Some("central asia")).await.unwrap();
        let codes: Vec<_> = s
            .data
            .instruments
            .iter()
            .map(|i| i.symbol.as_str())
            .collect();
        assert_eq!(codes, ["BBCA"]);
        assert_eq!(s.data.search.as_deref(), Some("central asia"));
    }

    #[tokio::test]
    async fn directories_fall_back_to_the_bundled_lists() {
        let (f, _be, _d) = fetcher(
            FixtureBackend::new()
                .route("GetSecuritiesStock", 403, "blocked")
                .route("GetBrokerSearch", 403, "blocked"),
        );
        let s = instruments(&f, None, Some("BBCA")).await.unwrap();
        assert_eq!(s.provider, "bundled");
        assert!(s.warnings[0].contains("Cloudflare"), "{:?}", s.warnings);
        assert_eq!(s.data.instruments[0].symbol.as_str(), "BBCA");
        assert_eq!(s.data.instruments[0].board, None);

        let m = members(&f, Some("cc")).await.unwrap();
        assert_eq!(m.provider, "bundled");
        assert!(m.data.members.iter().any(|x| x.code == "CC"));
        assert!(m.data.members.iter().all(|x| x.licenses.is_none()));
        assert_eq!(m.data.total, 89);
    }

    #[tokio::test]
    async fn members_from_idx_carry_licences() {
        let (f, _be, _d) = fetcher(FixtureBackend::new().route("GetBrokerSearch", 200, MEMBERS));
        let m = members(&f, None).await.unwrap();
        assert_eq!(m.data.members.len(), 89);
        assert!(m.data.members.windows(2).all(|w| w[0].code < w[1].code));
        assert!(m.data.members.iter().all(|x| x.licenses.is_some()));
    }

    #[tokio::test]
    async fn indices_walk_back_to_the_latest_session_and_filter_codes() {
        // Sunday 18 Oct: Saturday and Sunday are skipped without a request;
        // Friday 16 Oct has the data.
        let (f, be, _d) = fetcher(
            FixtureBackend::new()
                .route("GetIndexSummary?date=20261016", 200, INDICES)
                .route("GetIndexSummary", 200, NO_TRADING),
        );
        let s = indices(
            &f,
            None,
            &["lq45, ihsg".to_string(), "NOPE".to_string()],
            d(2026, 10, 18),
        )
        .await
        .unwrap();
        assert_eq!(s.data.date, d(2026, 10, 16));
        let codes: Vec<_> = s.data.indices.iter().map(|r| r.code.as_str()).collect();
        assert_eq!(codes, ["COMPOSITE", "LQ45"]);
        assert!(s.warnings.iter().any(|w| w.contains("NOPE")));
        assert_eq!(be.call_count(), 1);

        // Monday 19 Oct before the summary is out: falls back to Friday.
        let s = indices(&f, None, &[], d(2026, 10, 19)).await.unwrap();
        assert_eq!(s.data.date, d(2026, 10, 16));
        assert_eq!(s.data.indices.len(), 45);
        assert_eq!(be.calls_matching("date=20261019"), 1);

        let err = indices(&f, Some(d(2026, 8, 17)), &[], d(2026, 10, 19))
            .await
            .unwrap_err();
        assert_eq!(err.exit_code(), 4);
        assert!(err.to_string().contains("Proklamasi"), "{err}");
        let err = indices(&f, Some(d(2026, 10, 20)), &[], d(2026, 10, 19))
            .await
            .unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }

    const MONTH: &str = include_str!("../../tests/fixtures/idx/calendar_m_20261001_excerpt.json");
    const DAY: &str = include_str!("../../tests/fixtures/idx/calendar_d_20261019.json");

    #[tokio::test]
    async fn calendar_ranges_filter_by_date_and_default_categories() {
        let (f, be, _d) = fetcher(
            FixtureBackend::new()
                .route("range=m&date=20261001", 200, MONTH)
                .route("range=m&date=20261101", 200, "{\"Results\":[]}")
                .route("range=d&date=20261019", 200, DAY),
        );
        let q = CalendarQuery {
            from: Some(d(2026, 10, 15)),
            to: Some(d(2026, 10, 31)),
            ..Default::default()
        };
        let s = calendar(&f, &q, d(2026, 10, 18)).await.unwrap();
        assert_eq!(s.provider, "idx");
        assert_eq!(be.call_count(), 1, "one month");
        assert!(
            s.data.total > s.data.events.len(),
            "bonds and buybacks hidden"
        );
        assert!(s
            .data
            .events
            .iter()
            .all(|e| e.date >= d(2026, 10, 15) && e.date <= d(2026, 10, 31)));
        assert!(s.data.events.iter().all(|e| !matches!(
            e.category,
            CalendarCategory::Bond | CalendarCategory::Buyback
        )));
        assert!(s.data.events.windows(2).all(|w| w[0].date <= w[1].date));

        // Every category, across two months: one request per month.
        let q = CalendarQuery {
            from: Some(d(2026, 10, 20)),
            days: Some(20),
            categories: vec![CalendarCategory::Bond],
            ..Default::default()
        };
        let s = calendar(&f, &q, d(2026, 10, 18)).await.unwrap();
        assert_eq!(s.data.to, d(2026, 11, 9));
        assert!(!s.data.events.is_empty());
        assert!(s
            .data
            .events
            .iter()
            .all(|e| e.category == CalendarCategory::Bond));
        assert_eq!(be.calls_matching("date=20261101"), 1);

        // One day asks for the day.
        let q = CalendarQuery {
            date: Some(d(2026, 10, 19)),
            categories: CalendarCategory::ALL.to_vec(),
            ..Default::default()
        };
        let s = calendar(&f, &q, d(2026, 10, 18)).await.unwrap();
        assert_eq!(s.data.events.len(), 8);
        assert_eq!((s.data.from, s.data.to), (d(2026, 10, 19), d(2026, 10, 19)));
    }

    #[tokio::test]
    async fn calendar_passes_the_symbol_and_checks_the_range() {
        let (f, be, _d) =
            fetcher(FixtureBackend::new().route("GetCalendar", 200, "{\"Results\":[]}"));
        let q = CalendarQuery {
            symbol: Some("bbca".into()),
            ..Default::default()
        };
        let s = calendar(&f, &q, d(2026, 10, 18)).await.unwrap();
        assert_eq!(s.data.symbol.as_deref(), Some("BBCA"));
        assert!(s.data.events.is_empty());
        assert!(be.calls.lock().unwrap()[0].ends_with("&code=BBCA"));

        let too_long = CalendarQuery {
            from: Some(d(2026, 1, 1)),
            to: Some(d(2026, 6, 1)),
            ..Default::default()
        };
        assert_eq!(
            calendar(&f, &too_long, d(2026, 10, 18))
                .await
                .unwrap_err()
                .exit_code(),
            2
        );
        let backwards = CalendarQuery {
            from: Some(d(2026, 10, 20)),
            to: Some(d(2026, 10, 1)),
            ..Default::default()
        };
        assert!(calendar(&f, &backwards, d(2026, 10, 18)).await.is_err());
    }

    const ACTIONS: &str =
        include_str!("../../tests/fixtures/ipot/corporateaction_enrg_20250101.html");

    #[tokio::test]
    async fn corporate_actions_default_to_a_year_back_and_fill_the_name() {
        let (f, be, _d) = fetcher(FixtureBackend::new().route("corporateaction.php", 200, ACTIONS));
        let list = universe::load(None).unwrap();
        let sym = Symbol::parse("ENRG").unwrap();
        let s = corporate_actions(&f, &list, &sym, None, d(2026, 10, 18))
            .await
            .unwrap();
        assert_eq!(s.data.since, d(2025, 10, 18));
        assert!(be.calls.lock().unwrap()[0].ends_with("start=10/18/2025"));
        assert!(s.data.name.as_deref().unwrap().contains("ENERGI MEGA"));
        assert_eq!(s.data.warrants_rights[0].code, "ENRG-R");
        let idx = Symbol::parse("^JKSE").unwrap();
        assert!(corporate_actions(&f, &list, &idx, None, d(2026, 10, 18))
            .await
            .is_err());
    }
}
