//! Operations shared by the CLI and the MCP server: anything that combines a
//! provider call with local analysis lives here so both front ends behave
//! identically.

use std::sync::Arc;

use futures::future::join_all;
use futures::stream::{self, StreamExt};

use crate::analysis;
use crate::analysis::screen::{self as scr, Filter, Key};
use crate::error::{IdxError, Result};
use crate::http::Fetcher;
use crate::model::{
    BenchmarkReturns, DividendHistory, Fundamentals, History, Interval, MarketOverview, MarketRow,
    NewsItem, NewsReport, Performance, Period, Quote, ScreenReport, ScreenRow, SentimentReport,
    Symbol, Technical,
};
use crate::providers::news as newsfeed;
use crate::providers::{Provider, Sourced};
use crate::universe::Issuer;

/// Ratios plus the rule-based verdict.
pub async fn fundamentals(p: &dyn Provider, symbol: &Symbol) -> Result<Sourced<Fundamentals>> {
    let mut result = p.fundamentals(symbol).await?;
    result.data.verdict = Some(analysis::fundamental::verdict(&result.data));
    Ok(result)
}

/// Daily candles over `period`, run through the indicator set.
pub async fn technical(
    p: &dyn Provider,
    symbol: &Symbol,
    period: Period,
) -> Result<Sourced<Technical>> {
    let hist = p.history(symbol, period, Interval::Day1).await?;
    let Some(report) = analysis::technical::analyse(&hist.data) else {
        return Err(IdxError::NotFound(format!("no daily candles for {symbol}")));
    };
    Ok(Sourced {
        data: report,
        provider: hist.provider,
        cached: hist.cached,
        stale: hist.stale,
        fetched_at: hist.fetched_at,
        warnings: hist.warnings,
    })
}

/// Fundamentals for several symbols, fetched concurrently. Symbols that fail
/// become warnings; the call fails only when none succeed.
pub async fn compare(p: &dyn Provider, symbols: &[Symbol]) -> Result<Sourced<Vec<Fundamentals>>> {
    if symbols.len() < 2 {
        return Err(IdxError::Usage(
            "compare needs at least two different symbols".into(),
        ));
    }
    let results = join_all(symbols.iter().map(|s| fundamentals(p, s))).await;

    let mut out: Option<Sourced<Vec<Fundamentals>>> = None;
    let mut pending_warnings: Vec<String> = Vec::new();
    let mut first_err: Option<IdxError> = None;
    for (sym, r) in symbols.iter().zip(results) {
        match r {
            Ok(s) => match &mut out {
                None => out = Some(s.map(|d| vec![d])),
                Some(acc) => {
                    acc.absorb(&s);
                    acc.data.push(s.data);
                }
            },
            Err(e) => {
                let msg = match &e {
                    IdxError::NotFound(_) => format!("no data for {sym}"),
                    other => format!("{sym}: {other}"),
                };
                pending_warnings.push(msg);
                first_err.get_or_insert(e);
            }
        }
    }
    let Some(mut result) = out else {
        return Err(first_err.unwrap_or_else(|| IdxError::NotFound("no data".into())));
    };
    result.warnings.extend(pending_warnings);
    Ok(result)
}

/// Screener settings.
#[derive(Debug, Clone)]
pub struct ScreenOptions {
    pub filters: Vec<Filter>,
    pub sort: Key,
    pub descending: bool,
    /// 0 = all rows.
    pub limit: usize,
    /// Upper bound on per-symbol fundamentals calls (largest market caps first).
    pub max_fundamentals: usize,
    /// Fetch fundamentals even when no filter needs them.
    pub with_fundamentals: bool,
}

/// Quote-level screen over the universe, then a capped fundamentals pass.
pub async fn screen(
    p: &dyn Provider,
    universe: &[Issuer],
    opts: &ScreenOptions,
) -> Result<Sourced<ScreenReport>> {
    if universe.is_empty() {
        return Err(IdxError::Usage("the ticker universe is empty".into()));
    }
    let names: std::collections::HashMap<&Symbol, Option<&str>> = universe
        .iter()
        .map(|i| (&i.symbol, i.name.as_deref()))
        .collect();
    let symbols: Vec<Symbol> = universe.iter().map(|i| i.symbol.clone()).collect();

    // Quotes in batches; one failing batch fails the screen (it is the base data).
    let batches: Vec<&[Symbol]> = symbols.chunks(150).collect();
    let results = join_all(batches.iter().map(|b| p.quotes(b))).await;
    let mut out = Sourced::fresh(Vec::<ScreenRow>::new(), p.name());
    out.cached = true;
    let mut quoted = 0usize;
    for r in results {
        match r {
            Ok(s) => {
                out.absorb(&s);
                out.provider = s.provider;
                quoted += s.data.len();
                for q in &s.data {
                    let hint = names.get(&q.symbol).copied().flatten();
                    out.data.push(scr::row_from_quote(q, hint));
                }
            }
            Err(IdxError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }
    }
    out.warnings.retain(|w| !w.starts_with("no data for"));
    if quoted < symbols.len() {
        out.warnings.push(format!(
            "{} of {} tickers returned no quote and were skipped",
            symbols.len() - quoted,
            symbols.len()
        ));
    }

    let (quote_filters, fund_filters): (Vec<&Filter>, Vec<&Filter>) = opts
        .filters
        .iter()
        .partition(|f| !f.key.needs_fundamentals());
    let quote_filters: Vec<Filter> = quote_filters.into_iter().cloned().collect();
    let fund_filters: Vec<Filter> = fund_filters.into_iter().cloned().collect();
    let mut rows: Vec<ScreenRow> = out
        .data
        .iter()
        .filter(|r| scr::matches(r, &quote_filters))
        .cloned()
        .collect();
    let matched_quote_level = rows.len();

    let need_fund =
        opts.with_fundamentals || !fund_filters.is_empty() || opts.sort.needs_fundamentals();
    let mut checked = 0usize;
    let mut capped = false;
    if need_fund && !rows.is_empty() {
        scr::sort_rows(&mut rows, Key::Mcap, true);
        let cap = opts.max_fundamentals.max(1);
        if rows.len() > cap {
            capped = true;
            rows.truncate(cap);
        }
        let wanted: Vec<Symbol> = rows.iter().map(|r| r.symbol.clone()).collect();
        let fetched: Vec<(Symbol, Result<Sourced<Fundamentals>>)> = stream::iter(wanted)
            .map(|s| async move {
                let r = fundamentals(p, &s).await;
                (s, r)
            })
            .buffer_unordered(8)
            .collect()
            .await;
        for (sym, r) in fetched {
            match r {
                Ok(f) => {
                    checked += 1;
                    out.cached &= f.cached;
                    out.stale |= f.stale;
                    if let Some(row) = rows.iter_mut().find(|r| r.symbol == sym) {
                        scr::apply_fundamentals(row, &f.data);
                    }
                }
                Err(e) => tracing::debug!(symbol = %sym, error = %e, "fundamentals unavailable"),
            }
        }
        rows.retain(|r| scr::matches(r, &fund_filters));
        if capped {
            out.warnings.push(format!(
                "fundamentals were fetched for the {cap} largest companies that passed the quote filters; raise --max-fundamentals to widen"
            ));
        }
    }

    scr::sort_rows(&mut rows, opts.sort, opts.descending);
    let total = rows.len();
    if opts.limit > 0 {
        rows.truncate(opts.limit);
    }
    Ok(out.map(|_| ScreenReport {
        filters: opts.filters.iter().map(|f| f.raw.clone()).collect(),
        sort: opts.sort.name().to_string(),
        descending: opts.descending,
        universe: symbols.len(),
        quoted,
        matched_quote_level,
        fundamentals_checked: checked,
        fundamentals_capped: capped,
        total_matches: total,
        rows,
    }))
}

/// Quotes for a whole universe in batches. Batches that fail with NotFound
/// are skipped; any other failure is fatal because the quotes are the base
/// data. Returns the quotes and the number of symbols that answered.
pub async fn quotes_all(p: &dyn Provider, symbols: &[Symbol]) -> Result<Sourced<Vec<Quote>>> {
    let batches: Vec<&[Symbol]> = symbols.chunks(150).collect();
    let results = join_all(batches.iter().map(|b| p.quotes(b))).await;
    let mut out = Sourced::fresh(Vec::<Quote>::new(), p.name());
    out.cached = true;
    for r in results {
        match r {
            Ok(s) => {
                out.absorb(&s);
                out.provider = s.provider;
                out.data.extend(s.data);
            }
            Err(IdxError::NotFound(_)) => {}
            Err(e) => return Err(e),
        }
    }
    out.warnings.retain(|w| !w.starts_with("no data for"));
    if out.data.len() < symbols.len() {
        out.warnings.push(format!(
            "{} of {} tickers returned no quote and were skipped",
            symbols.len() - out.data.len(),
            symbols.len()
        ));
    }
    Ok(out)
}

/// The board: index quote, breadth and ranked lists from one pass over the
/// universe. `liquid_floor` is the market-cap floor for gainers/losers.
pub async fn market_overview(
    p: &dyn Provider,
    universe: &[Issuer],
    index: &Symbol,
    top: usize,
    liquid_floor: f64,
) -> Result<Sourced<MarketOverview>> {
    if universe.is_empty() {
        return Err(IdxError::Usage("the ticker universe is empty".into()));
    }
    let names: std::collections::HashMap<&Symbol, Option<&str>> = universe
        .iter()
        .map(|i| (&i.symbol, i.name.as_deref()))
        .collect();
    let symbols: Vec<Symbol> = universe.iter().map(|i| i.symbol.clone()).collect();
    let (quotes, index_q) = futures::join!(
        quotes_all(p, &symbols),
        p.quotes(std::slice::from_ref(index))
    );
    let mut out = quotes?;
    let mut index_row = None;
    let mut market_state = None;
    match index_q {
        Ok(s) => {
            out.absorb(&s);
            if let Some(q) = s.data.first() {
                market_state = q.market_state.clone();
                index_row = Some(MarketRow::from_quote(q, None));
            }
        }
        Err(e) => out.warnings.push(format!("index {index} unavailable: {e}")),
    }

    let rows: Vec<MarketRow> = out
        .data
        .iter()
        .filter(|q| q.price.is_some() && q.change_pct.is_some())
        .map(|q| MarketRow::from_quote(q, names.get(&q.symbol).copied().flatten()))
        .collect();
    let as_of = out.data.iter().filter_map(|q| q.market_time).max();
    let chg = |r: &MarketRow| r.change_pct.unwrap_or(0.0);
    let liquid: Vec<&MarketRow> = rows
        .iter()
        .filter(|r| r.market_cap.unwrap_or(0.0) >= liquid_floor && r.volume.unwrap_or(0) > 0)
        .collect();
    let mut gainers: Vec<&MarketRow> = liquid.iter().copied().filter(|r| chg(r) > 0.0).collect();
    gainers.sort_by(|a, b| {
        chg(b)
            .partial_cmp(&chg(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut losers: Vec<&MarketRow> = liquid.iter().copied().filter(|r| chg(r) < 0.0).collect();
    losers.sort_by(|a, b| {
        chg(a)
            .partial_cmp(&chg(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut by_value: Vec<&MarketRow> = rows.iter().filter(|r| r.value.is_some()).collect();
    by_value.sort_by(|a, b| {
        b.value
            .partial_cmp(&a.value)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut by_cap: Vec<&MarketRow> = rows.iter().filter(|r| r.market_cap.is_some()).collect();
    by_cap.sort_by(|a, b| {
        b.market_cap
            .partial_cmp(&a.market_cap)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let take =
        |v: Vec<&MarketRow>| -> Vec<MarketRow> { v.into_iter().take(top).cloned().collect() };
    let data = MarketOverview {
        as_of,
        market_state,
        index: index_row,
        universe: symbols.len(),
        quoted: rows.len(),
        advancing: rows.iter().filter(|r| chg(r) > 0.0).count(),
        declining: rows.iter().filter(|r| chg(r) < 0.0).count(),
        unchanged: rows.iter().filter(|r| chg(r) == 0.0).count(),
        turnover: rows.iter().filter_map(|r| r.value).sum(),
        liquid_min_market_cap: liquid_floor,
        gainers: take(gainers),
        losers: take(losers),
        most_active: take(by_value),
        heavyweights: take(by_cap),
    };
    Ok(out.map(|_| data))
}

/// Price performance from five years of daily candles for each symbol, with
/// excess returns over `benchmark` (an index such as `^JKSE`) when given.
/// Symbols that fail become warnings; the call fails only when none succeed.
pub async fn returns(
    p: &dyn Provider,
    symbols: &[Symbol],
    benchmark: Option<&Symbol>,
) -> Result<Sourced<Vec<Performance>>> {
    let (results, bench) = futures::join!(
        join_all(
            symbols
                .iter()
                .map(|s| p.history(s, Period::Y5, Interval::Day1))
        ),
        async {
            match benchmark {
                Some(b) => Some(p.history(b, Period::Y5, Interval::Day1).await),
                None => None,
            }
        }
    );
    let mut warnings: Vec<String> = Vec::new();
    let mut bench_perf: Option<(Sourced<History>, Performance)> = None;
    if let (Some(b), Some(r)) = (benchmark, bench) {
        match r {
            Ok(h) => match analysis::returns::analyse(&h.data) {
                Some(perf) => bench_perf = Some((h, perf)),
                None => warnings.push(format!("benchmark {b}: no candles")),
            },
            Err(e) => warnings.push(format!("benchmark {b} unavailable: {e}")),
        }
    }

    let mut out: Option<Sourced<Vec<Performance>>> = None;
    let mut first_err: Option<IdxError> = None;
    for (sym, r) in symbols.iter().zip(results) {
        match r {
            Ok(h) => {
                let Some(mut perf) = analysis::returns::analyse(&h.data) else {
                    warnings.push(format!("{sym}: no candles"));
                    continue;
                };
                if let Some((_, bp)) = &bench_perf {
                    perf.benchmark = Some(BenchmarkReturns {
                        symbol: bp.symbol.clone(),
                        name: bp.name.clone(),
                        returns: bp.returns.clone(),
                        excess: perf.returns.minus(&bp.returns),
                    });
                }
                match &mut out {
                    None => out = Some(h.map(|_| vec![perf])),
                    Some(acc) => {
                        acc.absorb(&h);
                        acc.data.push(perf);
                    }
                }
            }
            Err(e) => {
                warnings.push(match &e {
                    IdxError::NotFound(_) => format!("no data for {sym}"),
                    other => format!("{sym}: {other}"),
                });
                first_err.get_or_insert(e);
            }
        }
    }
    let Some(mut result) = out else {
        return Err(first_err.unwrap_or_else(|| IdxError::NotFound("no data".into())));
    };
    if let Some((bh, _)) = &bench_perf {
        result.absorb(bh);
    }
    result.warnings.extend(warnings);
    Ok(result)
}

/// Dividend and split history with totals, trailing yield and streak.
pub async fn dividends(p: &dyn Provider, symbol: &Symbol) -> Result<Sourced<DividendHistory>> {
    let mut h = p.dividends(symbol).await?;
    let today = chrono::Utc::now()
        .with_timezone(&crate::model::wib())
        .date_naive();
    analysis::dividends::enrich(&mut h.data, today);
    if h.data.dividends.is_empty() {
        h.warnings
            .push(format!("{symbol}: the source lists no dividend events"));
    }
    Ok(h)
}

/// Company name without legal-form tokens, for headline matching.
pub fn match_name(raw: &str) -> String {
    raw.split_whitespace()
        .map(|t| t.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|t| {
            !t.is_empty()
                && !matches!(
                    t.to_ascii_uppercase().as_str(),
                    "PT" | "TBK" | "PERSERO" | "LTD" | "INC"
                )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn item_matches(item: &NewsItem, ticker: &str, name: Option<&str>) -> Option<&'static str> {
    let text = format!("{} {}", item.title, item.summary);
    let upper = text.to_uppercase();
    let has_ticker = upper
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| tok == ticker);
    if has_ticker {
        return Some("ticker");
    }
    if let Some(n) = name {
        if n.len() >= 4 && upper.contains(&n.to_uppercase()) {
            return Some("name");
        }
    }
    None
}

/// Headlines from the selected feeds. With a symbol, the company's own
/// headlines come first (newest first) and the rest of `limit` is filled
/// with general market headlines so a reader always gets context; pass
/// `company_only` to leave those out.
pub async fn news(
    fetcher: &Arc<Fetcher>,
    universe: &[Issuer],
    symbol: Option<&Symbol>,
    sources: &[String],
    limit: usize,
    company_only: bool,
) -> Result<Sourced<NewsReport>> {
    let feeds = newsfeed::select(sources)?;
    let (items, warnings, cached, stale) = newsfeed::fetch(fetcher, &feeds).await?;
    let scanned = items.len();
    let matched_name = symbol.and_then(|s| {
        universe
            .iter()
            .find(|i| &i.symbol == s)
            .and_then(|i| i.name.as_deref())
            .map(match_name)
            .filter(|n| !n.is_empty())
    });
    let (mut items, company_matches) =
        arrange(items, symbol, matched_name.as_deref(), limit, company_only);
    for it in &mut items {
        analysis::sentiment::score_item(it);
    }
    let mut s = Sourced::fresh(
        NewsReport {
            symbol: symbol.cloned(),
            matched_name,
            sources: feeds.iter().map(|f| f.id.to_string()).collect(),
            scanned,
            company_matches,
            company_only: symbol.is_some() && company_only,
            items,
        },
        newsfeed::NAME,
    );
    s.cached = cached;
    s.stale = stale;
    s.warnings = warnings;
    Ok(s)
}

/// Splits scanned items into company and market scope and orders them:
/// company headlines first, then market context up to `limit` (0 = all).
/// Returns the arranged items and the number of company matches.
pub fn arrange(
    items: Vec<NewsItem>,
    symbol: Option<&Symbol>,
    matched_name: Option<&str>,
    limit: usize,
    company_only: bool,
) -> (Vec<NewsItem>, usize) {
    let Some(sym) = symbol else {
        let mut items = items;
        for it in &mut items {
            it.scope = "market".into();
        }
        if limit > 0 {
            items.truncate(limit);
        }
        return (items, 0);
    };
    let mut company = Vec::new();
    let mut market = Vec::new();
    for mut it in items {
        match item_matches(&it, sym.as_str(), matched_name) {
            Some(how) => {
                it.matched_by = how.into();
                it.scope = "company".into();
                company.push(it);
            }
            None => {
                it.matched_by = "all".into();
                it.scope = "market".into();
                market.push(it);
            }
        }
    }
    let company_matches = company.len();
    let mut out = company;
    if limit > 0 {
        out.truncate(limit);
    }
    if !company_only {
        let room = if limit > 0 {
            limit.saturating_sub(out.len())
        } else {
            market.len()
        };
        out.extend(market.into_iter().take(room));
    }
    (out, company_matches)
}

/// Lexicon sentiment over the headlines that mention the company only.
pub async fn sentiment(
    fetcher: &Arc<Fetcher>,
    universe: &[Issuer],
    symbol: &Symbol,
    sources: &[String],
    headline_limit: usize,
) -> Result<Sourced<SentimentReport>> {
    let n = news(fetcher, universe, Some(symbol), sources, 0, true).await?;
    let report = analysis::sentiment::aggregate(
        symbol,
        n.data.matched_name.clone(),
        &n.data.items,
        headline_limit,
    );
    Ok(n.map(|_| report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{History, Quote};

    #[test]
    fn match_names_and_items() {
        assert_eq!(match_name("PT Bank Central Asia Tbk"), "Bank Central Asia");
        assert_eq!(
            match_name("ADARO ANDALAN INDONESIA Tbk"),
            "ADARO ANDALAN INDONESIA"
        );
        let mk = |t: &str| NewsItem {
            title: t.into(),
            url: "u".into(),
            source: "s".into(),
            published: None,
            summary: String::new(),
            matched_by: String::new(),
            scope: String::new(),
            sentiment: 0.0,
            positive_terms: vec![],
            negative_terms: vec![],
        };
        assert_eq!(
            item_matches(&mk("Saham BBCA naik"), "BBCA", None),
            Some("ticker")
        );
        // Company headlines first, then market context fills the limit.
        let bbca = Symbol::parse("BBCA").unwrap();
        let feed = vec![
            mk("IHSG menguat"),
            mk("BBCA bagi dividen"),
            mk("Rupiah melemah"),
            mk("Laba BBCA naik"),
            mk("Harga emas"),
        ];
        let (items, matches) = arrange(feed.clone(), Some(&bbca), None, 3, false);
        assert_eq!(matches, 2);
        let got: Vec<(&str, &str)> = items
            .iter()
            .map(|i| (i.title.as_str(), i.scope.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("BBCA bagi dividen", "company"),
                ("Laba BBCA naik", "company"),
                ("IHSG menguat", "market"),
            ]
        );
        let (items, _) = arrange(feed.clone(), Some(&bbca), None, 3, true);
        assert_eq!(items.len(), 2);
        assert!(items.iter().all(|i| i.scope == "company"));
        let (items, matches) = arrange(feed, None, None, 0, false);
        assert_eq!((items.len(), matches), (5, 0));
        assert!(items.iter().all(|i| i.scope == "market"));
        assert_eq!(
            item_matches(&mk("Laba (BBCA) naik"), "BBCA", None),
            Some("ticker")
        );
        assert_eq!(
            item_matches(
                &mk("Bank Central Asia bagi dividen"),
                "BBCA",
                Some("Bank Central Asia")
            ),
            Some("name")
        );
        assert_eq!(item_matches(&mk("BBCAX bukan BBCA?"), "BBRI", None), None);
    }
    use async_trait::async_trait;

    struct Fake;

    #[async_trait]
    impl Provider for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }
        async fn quotes(&self, _: &[Symbol]) -> Result<Sourced<Vec<Quote>>> {
            unimplemented!()
        }
        async fn history(&self, _: &Symbol, _: Period, _: Interval) -> Result<Sourced<History>> {
            unimplemented!()
        }
        async fn fundamentals(&self, s: &Symbol) -> Result<Sourced<Fundamentals>> {
            if s.as_str().starts_with("ZZZZ") {
                return Err(IdxError::NotFound(s.to_string()));
            }
            let mut f = Fundamentals::empty(s.clone(), "fake");
            f.valuation.pe_trailing = Some(8.0);
            f.valuation.price_to_book = Some(0.9);
            f.profitability.roe_pct = Some(20.0);
            Ok(Sourced::fresh(f, "fake"))
        }
    }

    fn syms(list: &[&str]) -> Vec<Symbol> {
        list.iter().map(|s| Symbol::parse(s).unwrap()).collect()
    }

    #[tokio::test]
    async fn fundamentals_carry_a_verdict() {
        let f = fundamentals(&Fake, &Symbol::parse("BBCA").unwrap())
            .await
            .unwrap();
        assert!(f.data.verdict.is_some());
    }

    #[tokio::test]
    async fn compare_collects_warnings_and_keeps_order() {
        let c = compare(&Fake, &syms(&["ZZZZ", "BBCA", "BBRI"]))
            .await
            .unwrap();
        let got: Vec<&str> = c.data.iter().map(|f| f.symbol.as_str()).collect();
        assert_eq!(got, ["BBCA", "BBRI"]);
        assert_eq!(c.warnings, vec!["no data for ZZZZ".to_string()]);
        assert!(c.data.iter().all(|f| f.verdict.is_some()));

        let err = compare(&Fake, &syms(&["ZZZZ", "ZZZZ2"])).await.unwrap_err();
        assert_eq!(err.exit_code(), 4);
        let err = compare(&Fake, &syms(&["BBCA"])).await.unwrap_err();
        assert_eq!(err.exit_code(), 2);
    }
}
