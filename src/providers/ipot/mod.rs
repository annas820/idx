//! Indo Premier Sekuritas (IPOT) broker summary: which brokers bought and
//! sold a ticker.
//!
//! Endpoint verified 2026-10-17 (see `tests/fixtures/ipot/`), no login or key:
//! `https://www.indopremier.com/module/saham/include/data-brokersummary.php`
//! `?code=BBCA&start=MM/DD/YYYY&end=MM/DD/YYYY&fd=all|F|D&board=all|RG|TN|NG`
//! - Answers an HTML fragment with the top 10 buyers and top 10 sellers by
//!   value, each broker's class, and range totals.
//! - Figures of a million and more are abbreviated to one decimal (`1.6 M`,
//!   `841.2 B`, `2.2 T`); smaller ones are exact.
//! - A range without trades (weekend, holiday) gives ten blank rows and
//!   zero totals; an unknown code gives `Invalid stock code!` with HTTP 200.
//! - In some stretches of seconds the server answers nginx's own HTTP 404
//!   page to a third or more of requests, then recovers; the same URL then
//!   succeeds. Since a real unknown code is a 200, a 404 is retried up to
//!   [`ATTEMPTS`] times with doubling pauses (under 4 s in all).
//!
//! Broker names come from the bundled IDX member list (`crate::brokers`),
//! filled in by `service::broker`.

use std::sync::Arc;
use std::time::Duration;

use chrono::NaiveDate;

use crate::error::{IdxError, Result};
use crate::http::Fetcher;
use crate::model::{BrokerBoard, BrokerInvestor, BrokerSummary, Symbol};

use super::Sourced;

pub mod parse;

pub const NAME: &str = "ipot";
const ENDPOINT: &str = "https://www.indopremier.com/module/saham/include/data-brokersummary.php";

/// A range that ended before today no longer changes.
const TTL_CLOSED: Duration = Duration::from_secs(7 * 24 * 3600);
/// Today's summary moves with every trade.
const TTL_TODAY: Duration = Duration::from_secs(5 * 60);
/// Tries per request while the source answers its spurious 404.
pub const ATTEMPTS: usize = 5;
const RETRY_PAUSE: Duration = Duration::from_millis(if cfg!(test) { 1 } else { 250 });

pub fn url(
    symbol: &Symbol,
    from: NaiveDate,
    to: NaiveDate,
    investor: BrokerInvestor,
    board: BrokerBoard,
) -> String {
    let fd = match investor {
        BrokerInvestor::All => "all",
        BrokerInvestor::Foreign => "F",
        BrokerInvestor::Domestic => "D",
    };
    let board = match board {
        BrokerBoard::All => "all",
        BrokerBoard::Regular => "RG",
        BrokerBoard::Cash => "TN",
        BrokerBoard::Negotiated => "NG",
    };
    format!(
        "{ENDPOINT}?code={}&start={}&end={}&fd={fd}&board={board}",
        symbol.as_str(),
        from.format("%m/%d/%Y"),
        to.format("%m/%d/%Y"),
    )
}

/// Broker summary for `symbol` over `from..=to` (WIB dates). `today` decides
/// the cache lifetime.
pub async fn broker_summary(
    fetcher: &Arc<Fetcher>,
    symbol: &Symbol,
    from: NaiveDate,
    to: NaiveDate,
    investor: BrokerInvestor,
    board: BrokerBoard,
    today: NaiveDate,
) -> Result<Sourced<BrokerSummary>> {
    let url = url(symbol, from, to, investor, board);
    let ttl = if to < today { TTL_CLOSED } else { TTL_TODAY };
    let headers = [("Accept".to_string(), "text/html".to_string())];
    let mut f = fetcher.get(&url, &headers, ttl, None).await?;
    for attempt in 1..ATTEMPTS {
        if f.status != 404 {
            break;
        }
        tracing::debug!(%symbol, attempt, "ipot answered 404, retrying");
        tokio::time::sleep(RETRY_PAUSE * (1 << (attempt - 1))).await;
        f = fetcher.get(&url, &headers, ttl, None).await?;
    }
    if !f.is_success() {
        return Err(IdxError::provider(
            NAME,
            format!("broker summary for {symbol}: HTTP {}", f.status),
        ));
    }
    let p = match parse::parse(&f.body)? {
        parse::Page::Invalid => {
            return Err(IdxError::NotFound(format!(
                "{symbol} is not a stock code IPOT knows"
            )))
        }
        parse::Page::Summary(p) => p,
    };
    let mut s = Sourced::fresh(
        BrokerSummary {
            symbol: symbol.clone(),
            name: None,
            from: p.from.unwrap_or(from),
            to: p.to.unwrap_or(to),
            investor,
            board,
            buyers: p.buyers,
            sellers: p.sellers,
            total_value: p.total_value,
            total_lots: p.total_lots,
            foreign_net_value: p.foreign_net_value,
            average_price: p.average_price,
            rounded: p.rounded,
        },
        NAME,
    );
    s.cached = f.cached;
    s.stale = f.stale;
    s.fetched_at = f.fetched_at;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Cache;
    use crate::http::{CacheMode, FixtureBackend};

    const DAY: &str = include_str!("../../../tests/fixtures/ipot/brokersummary_bbca_20261016.html");
    const INVALID: &str =
        include_str!("../../../tests/fixtures/ipot/brokersummary_zzzz_invalid.html");

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn fetcher(be: FixtureBackend) -> (Arc<Fetcher>, Arc<FixtureBackend>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let be = Arc::new(be);
        let f = Arc::new(Fetcher::new(
            be.clone(),
            Cache::new(dir.path()),
            CacheMode::Normal,
        ));
        (f, be, dir)
    }

    #[test]
    fn url_maps_filters_to_source_codes() {
        let s = Symbol::parse("bbca").unwrap();
        let u = url(
            &s,
            d(2026, 10, 12),
            d(2026, 10, 16),
            BrokerInvestor::Foreign,
            BrokerBoard::Regular,
        );
        assert_eq!(
            u,
            "https://www.indopremier.com/module/saham/include/data-brokersummary.php?code=BBCA&start=10/12/2026&end=10/16/2026&fd=F&board=RG"
        );
    }

    #[tokio::test]
    async fn fetches_parses_and_caches() {
        let (f, be, _d) =
            fetcher(FixtureBackend::new().route("data-brokersummary.php?code=BBCA", 200, DAY));
        let s = Symbol::parse("BBCA").unwrap();
        let day = d(2026, 10, 16);
        let got = broker_summary(
            &f,
            &s,
            day,
            day,
            BrokerInvestor::All,
            BrokerBoard::All,
            d(2026, 10, 17),
        )
        .await
        .unwrap();
        assert_eq!(got.provider, "ipot");
        assert_eq!(got.data.buyers[0].code, "MG");
        assert_eq!((got.data.from, got.data.to), (day, day));
        assert!(!got.cached);
        let again = broker_summary(
            &f,
            &s,
            day,
            day,
            BrokerInvestor::All,
            BrokerBoard::All,
            d(2026, 10, 17),
        )
        .await
        .unwrap();
        assert!(again.cached);
        assert_eq!(be.call_count(), 1);
    }

    #[tokio::test]
    async fn unknown_code_is_not_found_and_http_errors_are_provider_errors() {
        let (f, be, _d) = fetcher(
            FixtureBackend::new()
                .route("code=ZZZZ", 200, INVALID)
                .route("code=BBRI", 503, "busy")
                .route("code=TLKM", 404, "Not Found"),
        );
        let day = d(2026, 10, 16);
        let err = broker_summary(
            &f,
            &Symbol::parse("ZZZZ").unwrap(),
            day,
            day,
            BrokerInvestor::All,
            BrokerBoard::All,
            day,
        )
        .await
        .unwrap_err();
        assert_eq!(err.exit_code(), 4);
        let err = broker_summary(
            &f,
            &Symbol::parse("BBRI").unwrap(),
            day,
            day,
            BrokerInvestor::All,
            BrokerBoard::All,
            day,
        )
        .await
        .unwrap_err();
        assert_eq!(err.exit_code(), 3);
        assert_eq!(be.call_count(), 2, "only the spurious 404 is retried");

        let err = broker_summary(
            &f,
            &Symbol::parse("TLKM").unwrap(),
            day,
            day,
            BrokerInvestor::All,
            BrokerBoard::All,
            day,
        )
        .await
        .unwrap_err();
        assert_eq!(err.exit_code(), 3);
        assert!(err.to_string().contains("HTTP 404"), "{err}");
        assert_eq!(be.call_count(), 2 + ATTEMPTS);
    }
}
