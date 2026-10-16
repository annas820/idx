//! Yahoo Finance provider.
//!
//! Endpoints (all unofficial, no key):
//! - `GET https://query1.finance.yahoo.com/v8/finance/chart/{SYM}.JK?range=..&interval=..`
//!   No auth. Used for `history` and as the quote fallback.
//! - `GET https://query1.finance.yahoo.com/v7/finance/quote?symbols=A.JK,B.JK&crumb=..`
//!   Needs the `A3` cookie from `https://fc.yahoo.com` plus the crumb from
//!   `https://query2.finance.yahoo.com/v1/test/getcrumb`. Returns the rich quote
//!   row (previous close, market cap, P/E, dividend yield, ...).
//!
//! Recorded responses live in `tests/fixtures/yahoo/`.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::future::join_all;

use crate::error::{IdxError, Result};
use crate::http::Fetcher;
use crate::model::{History, Interval, Period, Quote, Symbol};

use super::{Provider, Sourced, Ttls};

mod parse;
mod session;

pub use parse::{parse_chart, parse_chart_quote, parse_v7_quotes};

pub const NAME: &str = "yahoo";
const CHART_BASE: &str = "https://query1.finance.yahoo.com/v8/finance/chart/";
const QUOTE_URL: &str = "https://query1.finance.yahoo.com/v7/finance/quote";
/// Intraday candles move constantly; cache them briefly regardless of config.
const INTRADAY_TTL: Duration = Duration::from_secs(120);

pub struct Yahoo {
    fetcher: Arc<Fetcher>,
    ttls: Ttls,
}

impl Yahoo {
    pub fn new(fetcher: Arc<Fetcher>, ttls: Ttls) -> Self {
        Yahoo { fetcher, ttls }
    }

    pub fn chart_url(symbol: &Symbol, period: Period, interval: Interval) -> String {
        format!(
            "{CHART_BASE}{}?range={}&interval={}&includePrePost=false",
            symbol.yahoo(),
            period.as_str(),
            interval.as_str()
        )
    }

    fn quote_cache_key(symbols: &[Symbol]) -> String {
        let list: Vec<String> = symbols.iter().map(Symbol::yahoo).collect();
        format!("{QUOTE_URL}?symbols={}", list.join(","))
    }

    /// Rich quotes via the v7 endpoint (needs a session).
    async fn quotes_v7(&self, symbols: &[Symbol]) -> Result<Sourced<Vec<Quote>>> {
        let key = Self::quote_cache_key(symbols);
        let mut session = session::get(&self.fetcher, false).await?;

        for attempt in 0..2 {
            let (url, headers) = match &session {
                Some(s) => (format!("{key}&crumb={}", urlencode(&s.crumb)), s.headers()),
                // Offline with no session: only a cache hit can satisfy this.
                None => (key.clone(), vec![]),
            };
            let fetched = self
                .fetcher
                .get(&url, &headers, self.ttls.quote, Some(&key))
                .await?;

            if fetched.status == 401 || fetched.status == 403 {
                if attempt == 0 && !self.fetcher.is_offline() {
                    tracing::info!("yahoo session rejected, refreshing crumb");
                    session = session::get(&self.fetcher, true).await?;
                    continue;
                }
                return Err(IdxError::provider(
                    NAME,
                    "quote endpoint rejected the session",
                ));
            }
            if !fetched.is_success() {
                return Err(IdxError::provider(
                    NAME,
                    format!("quote endpoint returned HTTP {}", fetched.status),
                ));
            }

            let quotes = parse_v7_quotes(&fetched.body)?;
            let mut warnings = Vec::new();
            for s in symbols {
                if !quotes.iter().any(|q| &q.symbol == s) {
                    warnings.push(format!("no data for {s}"));
                }
            }
            if quotes.is_empty() {
                let list: Vec<&str> = symbols.iter().map(Symbol::as_str).collect();
                return Err(IdxError::NotFound(format!(
                    "no quotes for {}",
                    list.join(", ")
                )));
            }
            return Ok(Sourced {
                data: quotes,
                provider: NAME,
                cached: fetched.cached,
                stale: fetched.stale,
                fetched_at: fetched.fetched_at,
                warnings,
            });
        }
        unreachable!("loop returns on the second attempt")
    }

    /// Reduced quotes assembled from the chart endpoint, one request per symbol.
    async fn quotes_chart(&self, symbols: &[Symbol]) -> Result<Sourced<Vec<Quote>>> {
        let futs = symbols.iter().map(|s| async move {
            let url = Self::chart_url(s, Period::D5, Interval::Day1);
            let f = self.fetcher.get(&url, &[], self.ttls.quote, None).await?;
            if f.status == 404 {
                return Err(IdxError::NotFound(format!("{s}")));
            }
            if !f.is_success() {
                return Err(IdxError::provider(
                    NAME,
                    format!("chart endpoint HTTP {}", f.status),
                ));
            }
            let q = parse_chart_quote(&f.body)?;
            Ok::<_, IdxError>((q, f.cached, f.stale, f.fetched_at))
        });
        let results = join_all(futs).await;

        let mut out = Sourced::fresh(Vec::new(), NAME);
        out.cached = true;
        let mut first_err: Option<IdxError> = None;
        for (sym, r) in symbols.iter().zip(results) {
            match r {
                Ok((q, cached, stale, at)) => {
                    out.cached &= cached;
                    out.stale |= stale;
                    out.fetched_at = out.fetched_at.min(at);
                    out.data.push(q);
                }
                Err(IdxError::NotFound(_)) => out.warnings.push(format!("no data for {sym}")),
                Err(e) => {
                    out.warnings.push(format!("{sym}: {e}"));
                    first_err.get_or_insert(e);
                }
            }
        }
        if out.data.is_empty() {
            if let Some(e) = first_err {
                return Err(e);
            }
            let list: Vec<&str> = symbols.iter().map(Symbol::as_str).collect();
            return Err(IdxError::NotFound(format!(
                "no quotes for {}",
                list.join(", ")
            )));
        }
        Ok(out)
    }
}

#[async_trait]
impl Provider for Yahoo {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn quotes(&self, symbols: &[Symbol]) -> Result<Sourced<Vec<Quote>>> {
        match self.quotes_v7(symbols).await {
            Ok(q) => Ok(q),
            Err(e @ IdxError::NotFound(_)) => Err(e),
            Err(e) => {
                tracing::info!(error = %e, "yahoo quote endpoint failed, falling back to chart");
                let mut fallback = match self.quotes_chart(symbols).await {
                    Ok(f) => f,
                    Err(IdxError::OfflineMiss(_)) => {
                        let list: Vec<&str> = symbols.iter().map(Symbol::as_str).collect();
                        return Err(IdxError::OfflineMiss(format!(
                            "no cached quote for {}",
                            list.join(", ")
                        )));
                    }
                    Err(other) => return Err(other),
                };
                fallback.warnings.insert(
                    0,
                    format!("quote endpoint unavailable ({e}); reduced fields from chart"),
                );
                Ok(fallback)
            }
        }
    }

    async fn history(
        &self,
        symbol: &Symbol,
        period: Period,
        interval: Interval,
    ) -> Result<Sourced<History>> {
        let url = Self::chart_url(symbol, period, interval);
        let ttl = if interval.is_intraday() {
            INTRADAY_TTL.min(self.ttls.history)
        } else {
            self.ttls.history
        };
        let f = match self.fetcher.get(&url, &[], ttl, None).await {
            Ok(f) => f,
            Err(IdxError::OfflineMiss(_)) => {
                return Err(IdxError::OfflineMiss(format!(
                    "no cached {period}/{interval} history for {symbol}"
                )))
            }
            Err(e) => return Err(e),
        };
        if f.status == 404 {
            return Err(IdxError::NotFound(format!(
                "{symbol}: {}",
                parse::chart_error(&f.body)
            )));
        }
        if !f.is_success() {
            return Err(IdxError::provider(
                NAME,
                format!(
                    "chart endpoint HTTP {}: {}",
                    f.status,
                    parse::chart_error(&f.body)
                ),
            ));
        }
        let history = parse_chart(&f.body, period, interval)?;
        Ok(Sourced {
            data: history,
            provider: NAME,
            cached: f.cached,
            stale: f.stale,
            fetched_at: f.fetched_at,
            warnings: vec![],
        })
    }
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Cache;
    use crate::http::{CacheMode, FixtureBackend};

    const V7: &str = include_str!("../../../tests/fixtures/yahoo/quote_v7_bbca_bbri_tlkm.json");
    const V7_PARTIAL: &str =
        include_str!("../../../tests/fixtures/yahoo/quote_v7_partial_bbca_zzzz.json");
    const V7_401: &str = include_str!("../../../tests/fixtures/yahoo/quote_v7_unauthorized.json");
    const CHART_1MO: &str = include_str!("../../../tests/fixtures/yahoo/chart_bbca_1mo_1d.json");
    const CHART_404: &str = include_str!("../../../tests/fixtures/yahoo/chart_not_found.json");

    fn session_routes(be: FixtureBackend) -> FixtureBackend {
        be.route_with_headers(
            "https://fc.yahoo.com",
            404,
            "",
            &[(
                "set-cookie",
                "A3=d=abc&S=xyz; Expires=Fri; Domain=.yahoo.com; Secure; HttpOnly",
            )],
        )
        .route("v1/test/getcrumb", 200, "crumb123")
    }

    fn yahoo(
        be: FixtureBackend,
        mode: CacheMode,
    ) -> (Yahoo, Arc<FixtureBackend>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let be = Arc::new(be);
        let fetcher = Arc::new(Fetcher::new(be.clone(), Cache::new(dir.path()), mode));
        (Yahoo::new(fetcher, Ttls::default()), be, dir)
    }

    fn syms(list: &[&str]) -> Vec<Symbol> {
        list.iter().map(|s| Symbol::parse(s).unwrap()).collect()
    }

    #[tokio::test]
    async fn quotes_via_v7_with_session() {
        let be = session_routes(FixtureBackend::new()).route("v7/finance/quote", 200, V7);
        let (y, be, _d) = yahoo(be, CacheMode::Normal);
        let got = y.quotes(&syms(&["BBCA", "BBRI", "TLKM"])).await.unwrap();
        assert_eq!(got.provider, "yahoo");
        assert_eq!(got.data.len(), 3);
        assert_eq!(got.data[0].symbol.as_str(), "BBCA");
        assert_eq!(got.data[0].price, Some(5175.0));
        assert_eq!(got.data[0].previous_close, Some(6075.0));
        assert!(got.warnings.is_empty());
        // fc + crumb + quote
        assert_eq!(be.call_count(), 3);
        {
            let calls = be.calls.lock().unwrap();
            assert!(calls[2].contains("crumb=crumb123"), "{}", calls[2]);
        }

        // Second call: session and quote both come from cache -> no network.
        let again = y.quotes(&syms(&["BBCA", "BBRI", "TLKM"])).await.unwrap();
        assert!(again.cached);
        assert_eq!(be.call_count(), 3);
    }

    #[tokio::test]
    async fn unknown_symbol_is_a_warning_when_others_exist() {
        let be = session_routes(FixtureBackend::new()).route("v7/finance/quote", 200, V7_PARTIAL);
        let (y, _be, _d) = yahoo(be, CacheMode::Normal);
        let got = y.quotes(&syms(&["BBCA", "ZZZZ"])).await.unwrap();
        assert_eq!(got.data.len(), 1);
        assert_eq!(got.warnings, vec!["no data for ZZZZ".to_string()]);
    }

    #[tokio::test]
    async fn v7_rejection_refreshes_then_falls_back_to_chart() {
        let be = session_routes(FixtureBackend::new())
            .route("v7/finance/quote", 401, V7_401)
            .route("v8/finance/chart/BBCA.JK", 200, CHART_1MO)
            .route("v8/finance/chart/ZZZZ.JK", 404, CHART_404);
        let (y, be, _d) = yahoo(be, CacheMode::Normal);
        let got = y.quotes(&syms(&["BBCA", "ZZZZ"])).await.unwrap();
        assert_eq!(got.data.len(), 1);
        assert_eq!(got.data[0].source, "yahoo:chart");
        assert_eq!(got.data[0].price, Some(5175.0));
        assert!(got.warnings.iter().any(|w| w.contains("no data for ZZZZ")));
        assert!(got.warnings[0].contains("reduced fields"));
        let calls = be.calls.lock().unwrap();
        // fc, crumb, quote(401), fc, crumb, quote(401), chart x2
        assert_eq!(calls.iter().filter(|u| u.contains("getcrumb")).count(), 2);
        assert_eq!(
            calls
                .iter()
                .filter(|u| u.contains("v8/finance/chart"))
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn all_unknown_is_not_found() {
        let be = session_routes(FixtureBackend::new()).route(
            "v7/finance/quote",
            200,
            r#"{"quoteResponse":{"result":[],"error":null}}"#,
        );
        let (y, _be, _d) = yahoo(be, CacheMode::Normal);
        let err = y.quotes(&syms(&["ZZZZ"])).await.unwrap_err();
        assert_eq!(err.exit_code(), 4);
    }

    #[tokio::test]
    async fn history_parses_and_maps_404() {
        let be = FixtureBackend::new()
            .route("v8/finance/chart/BBCA.JK", 200, CHART_1MO)
            .route("v8/finance/chart/ZZZZ.JK", 404, CHART_404);
        let (y, _be, _d) = yahoo(be, CacheMode::Normal);
        let h = y
            .history(&Symbol::parse("BBCA").unwrap(), Period::M1, Interval::Day1)
            .await
            .unwrap();
        assert_eq!(h.data.candles.len(), 22);
        assert_eq!(h.data.summary.as_ref().unwrap().last_close, 6075.0);
        let err = y
            .history(&Symbol::parse("ZZZZ").unwrap(), Period::M1, Interval::Day1)
            .await
            .unwrap_err();
        assert_eq!(err.exit_code(), 4);
        assert!(err.to_string().contains("delisted"), "{err}");
    }

    #[tokio::test]
    async fn offline_without_cache_is_exit_5() {
        let (y, _be, _d) = yahoo(FixtureBackend::new(), CacheMode::Offline);
        let err = y.quotes(&syms(&["BBCA"])).await.unwrap_err();
        assert_eq!(err.exit_code(), 5, "{err}");
    }

    #[test]
    fn url_building() {
        let u = Yahoo::chart_url(&Symbol::parse("bbca").unwrap(), Period::Y1, Interval::Week1);
        assert_eq!(
            u,
            "https://query1.finance.yahoo.com/v8/finance/chart/BBCA.JK?range=1y&interval=1wk&includePrePost=false"
        );
        assert_eq!(urlencode("a/b c"), "a%2Fb%20c");
    }
}
