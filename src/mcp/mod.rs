//! `idx mcp`: the same capabilities as the CLI, exposed as MCP tools over
//! stdio. Every tool returns the exact JSON envelope `idx -o json` prints
//! (see `docs/json-output.md`), so an agent that learned one learned both.
//! Failures come back as tool errors carrying the same `{error, exit_code}`
//! object the CLI writes to stderr.
//!
//! Nothing here may write to stdout except the protocol; logs go to stderr.

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolResult, ContentBlock, Implementation, ListToolsResult, PaginatedRequestParams,
    ProtocolVersion, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{
    schemars, tool, tool_handler, tool_router, ErrorData as McpError, RoleServer, ServerHandler,
};
use serde::Deserialize;

use crate::error::{IdxError, Result};
use crate::http::Fetcher;
use crate::model::{Interval, Period, StatementPeriod, Symbol};
use crate::output::json::{
    self, Envelope, ErrorEnvelope, SCHEMA_BRIEF, SCHEMA_COMPARE, SCHEMA_DIVIDENDS, SCHEMA_EARNINGS,
    SCHEMA_FINANCIALS, SCHEMA_FUNDAMENTALS, SCHEMA_HISTORY, SCHEMA_MARKET,
    SCHEMA_OWNERSHIP_CHANGES, SCHEMA_OWNERSHIP_ENTITY, SCHEMA_OWNERSHIP_FOREIGN,
    SCHEMA_OWNERSHIP_FOREIGN_FLOW, SCHEMA_OWNERSHIP_HOLDERS, SCHEMA_OWNERSHIP_RELEASES,
    SCHEMA_PROFILE, SCHEMA_QUOTES, SCHEMA_RETURNS, SCHEMA_TECHNICAL,
};
use crate::output::json::{SCHEMA_NEWS, SCHEMA_SCREEN, SCHEMA_SENTIMENT};
use crate::ownership::OwnershipDb;
use crate::providers::{Provider, Sourced};
use crate::service;
use crate::universe;

pub mod schema;

const INSTRUCTIONS: &str = "\
idx: Indonesian stock market (IDX / IHSG) data and analysis from free public sources.
Tickers are IDX codes such as BBCA, BBRI, TLKM (no .JK suffix needed).
Every tool returns the same JSON envelope as the `idx -o json` CLI:
{schema, provider, fetched_at, cached, stale, warnings?, data}. Fields the source did
not return are null; nothing is estimated. Prices are delayed as published.
Errors return isError=true with {error:{code,message}, exit_code}; code not_found
means the ticker is unknown, offline_miss means --offline with nothing cached.
Ownership tools read the local SQLite register of holders above 1% (KSEI via IDX,
monthly); if it is empty they say so and the user must run `idx ownership import --latest`.
Verdicts and signals are transparent rule scores, not investment advice.";

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SymbolsArg {
    /// One or more IDX tickers, comma or space separated, e.g. "BBCA,BBRI,TLKM".
    pub symbols: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SymbolArg {
    /// One IDX ticker, e.g. "BBCA".
    pub symbol: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct HistoryArgs {
    /// One IDX ticker, e.g. "TLKM".
    pub symbol: String,
    /// Look-back window: 1d, 5d, 1mo, 3mo, 6mo, 1y (default), 2y, 5y, 10y, ytd, max.
    #[serde(default)]
    pub period: Option<String>,
    /// Candle size: 5m, 15m, 30m, 1h, 1d (default), 1wk, 1mo. Intraday sizes need short periods.
    #[serde(default)]
    pub interval: Option<String>,
    /// Keep only the most recent N candles (default 60). 0 returns all of them.
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct FinancialsArgs {
    /// One IDX ticker, e.g. "BBCA".
    pub symbol: String,
    /// annual (default, ~4 fiscal years), quarterly (~5 quarters) or ttm (trailing twelve months).
    #[serde(default)]
    pub period: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TechnicalArgs {
    /// One IDX ticker, e.g. "BBRI".
    pub symbol: String,
    /// Daily candles to analyse: 6mo, 1y (default), 2y, 5y. Shorter windows leave the 200-day averages null.
    #[serde(default)]
    pub period: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OwnershipTickerArgs {
    /// One IDX ticker, e.g. "BBCA".
    pub symbol: String,
    /// Release date YYYY-MM-DD (default: newest imported). idx_ownership_releases lists them.
    #[serde(default)]
    pub as_of: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OwnershipEntityArgs {
    /// Part of a holder's name (person or institution), at least 3 characters, e.g. "Dwimuria".
    pub name: String,
    /// Release date YYYY-MM-DD (default: newest imported).
    #[serde(default)]
    pub as_of: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OwnershipChangesArgs {
    /// Earlier release date YYYY-MM-DD.
    pub from: String,
    /// Later release date YYYY-MM-DD.
    pub to: String,
    /// Restrict to one ticker.
    #[serde(default)]
    pub symbol: Option<String>,
    /// Ignore moves smaller than this many percentage points (default 0).
    #[serde(default)]
    pub min_pct_change: Option<f64>,
    /// Rows per page, largest moves first (default 50; 0 = all).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Rows to skip before the page (default 0). total_matches and offset in the result tell you when to stop.
    #[serde(default)]
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct OwnershipForeignArgs {
    /// One IDX ticker for its month-by-month foreign share; omit for the market-wide flow between two releases.
    #[serde(default)]
    pub symbol: Option<String>,
    /// Earlier release YYYY-MM-DD for the market-wide flow (default: the release before `to`).
    #[serde(default)]
    pub from: Option<String>,
    /// Later release YYYY-MM-DD (default: newest imported).
    #[serde(default)]
    pub to: Option<String>,
    /// Rows in the market-wide flow (default 25; 0 = all).
    #[serde(default)]
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ScreenArgs {
    /// Comma-separated clauses such as "pe<15,roe>15,sector=financial,mcap>10T". Keys: pe fpe pb ps peg mcap price div eps volume chg roe roa margin opmargin de growth epsgrowth sector industry. Operators < <= > >= = !=; numbers accept K/M/B/T and %.
    pub filter: String,
    /// Sort key (any filter key), default mcap, descending.
    #[serde(default)]
    pub sort: Option<String>,
    /// Rows returned (default 25, 0 = all).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Cap on per-company fundamentals fetches when a filter needs them (default 50).
    #[serde(default)]
    pub max_fundamentals: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct NewsArgs {
    /// Optional IDX ticker; omit for general market headlines.
    #[serde(default)]
    pub symbol: Option<String>,
    /// Publisher names: kontan, cnbcindonesia, bisnis, all (default: kontan + cnbcindonesia).
    #[serde(default)]
    pub sources: Option<Vec<String>>,
    /// Headlines returned (default 20, 0 = all).
    #[serde(default)]
    pub limit: Option<u32>,
    /// With a symbol: return only headlines that mention the company, no market context (default false).
    #[serde(default)]
    pub company_only: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct MarketArgs {
    /// Rows per list (default 10).
    #[serde(default)]
    pub top: Option<u32>,
    /// Market-cap floor in IDR for gainers/losers (default 1e12 = 1T).
    #[serde(default)]
    pub min_market_cap: Option<f64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ReturnsArgs {
    /// One or more IDX tickers, comma or space separated, e.g. "BBCA,BBRI".
    pub symbols: String,
    /// Index for excess returns: "^JKSE" (IHSG, default), "^JKLQ45", or "none" to skip it.
    #[serde(default)]
    pub benchmark: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SentimentArgs {
    /// One IDX ticker.
    pub symbol: String,
    #[serde(default)]
    pub sources: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct IdxServer {
    provider: Arc<dyn Provider>,
    fetcher: Arc<Fetcher>,
    ownership_db: PathBuf,
    tool_router: ToolRouter<IdxServer>,
}

fn parse_date(v: &Option<String>) -> Result<Option<chrono::NaiveDate>> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(Some)
            .map_err(|_| IdxError::Usage(format!("invalid date {s:?}; use YYYY-MM-DD"))),
    }
}

fn ok<T: serde::Serialize>(schema: &str, s: Sourced<T>) -> Result<CallToolResult> {
    let env = Envelope::from_sourced(schema, s);
    Ok(CallToolResult::success(vec![ContentBlock::text(
        json::to_pretty(&env)?,
    )]))
}

/// Tool-level failure: the agent sees the same error object the CLI prints.
fn fail(e: IdxError) -> CallToolResult {
    let env = ErrorEnvelope::from(&e);
    let text = serde_json::to_string_pretty(&env).unwrap_or_else(|_| e.to_string());
    CallToolResult::error(vec![ContentBlock::text(text)])
}

fn finish(r: Result<CallToolResult>) -> std::result::Result<CallToolResult, McpError> {
    Ok(r.unwrap_or_else(fail))
}

fn parse_opt<T: std::str::FromStr<Err = IdxError>>(v: &Option<String>, default: T) -> Result<T> {
    match v.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => s.parse(),
        None => Ok(default),
    }
}

#[tool_router]
impl IdxServer {
    pub fn new(provider: Arc<dyn Provider>, fetcher: Arc<Fetcher>, ownership_db: PathBuf) -> Self {
        IdxServer {
            provider,
            fetcher,
            ownership_db,
            tool_router: Self::tool_router(),
        }
    }

    fn db(&self) -> Result<OwnershipDb> {
        OwnershipDb::open(&self.ownership_db)
    }

    /// Names of the tools, for tests and docs.
    pub fn tool_names(&self) -> Vec<String> {
        self.tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect()
    }

    /// The tool list with input schemas rewritten into the subset every
    /// harness accepts (no `$schema`, no `format`, no nullable type arrays).
    pub fn tools(&self) -> Vec<Tool> {
        self.tool_router
            .list_all()
            .into_iter()
            .map(|mut t| {
                let mut v = serde_json::Value::Object((*t.input_schema).clone());
                schema::portable(&mut v);
                if let serde_json::Value::Object(m) = v {
                    t.input_schema = Arc::new(m);
                }
                t
            })
            .collect()
    }

    #[tool(
        name = "idx_quote",
        description = "Delayed quotes for one or more IDX tickers: price, change, previous close, OHLC, volume, market cap, P/E, dividend yield, 52-week range. Returns the idx.quotes.v1 envelope; unknown tickers are listed in warnings."
    )]
    async fn quote(
        &self,
        Parameters(a): Parameters<SymbolsArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbols = Symbol::parse_list([a.symbols.as_str()])?;
                ok(SCHEMA_QUOTES, self.provider.quotes(&symbols).await?)
            }
            .await,
        )
    }

    #[tool(
        name = "idx_history",
        description = "OHLCV candles for one IDX ticker, oldest first, with a summary (first/last close, change %, high, low, average volume). Returns the idx.history.v1 envelope. Use limit to bound the payload; the summary always covers the whole period."
    )]
    async fn history(
        &self,
        Parameters(a): Parameters<HistoryArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                let period = parse_opt(&a.period, Period::Y1)?;
                let interval = parse_opt(&a.interval, Interval::Day1)?;
                let mut h = self.provider.history(&symbol, period, interval).await?;
                let limit = a.limit.unwrap_or(60) as usize;
                if limit > 0 && h.data.candles.len() > limit {
                    let skip = h.data.candles.len() - limit;
                    h.data.candles.drain(..skip);
                    h.warnings.push(format!(
                        "candles truncated to the last {limit}; summary covers the full period"
                    ));
                }
                ok(SCHEMA_HISTORY, h)
            }
            .await,
        )
    }

    #[tool(
        name = "idx_profile",
        description = "Company profile for one IDX ticker: name, sector, industry, business description, employees, founding year, headquarters, website, market cap, shares outstanding, board and management. Returns the idx.profile.v1 envelope."
    )]
    async fn profile(
        &self,
        Parameters(a): Parameters<SymbolArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                ok(SCHEMA_PROFILE, self.provider.profile(&symbol).await?)
            }
            .await,
        )
    }

    #[tool(
        name = "idx_financials",
        description = "Income statement, balance sheet and cash flow for one IDX ticker, oldest period first, amounts in reporting currency (IDR). Returns the idx.financials.v1 envelope. Banks have no EBITDA/gross profit series (null)."
    )]
    async fn financials(
        &self,
        Parameters(a): Parameters<FinancialsArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                let period = match a.period.as_deref().map(str::trim) {
                    None | Some("") | Some("annual") => StatementPeriod::Annual,
                    Some("quarterly") => StatementPeriod::Quarterly,
                    Some("ttm") => StatementPeriod::Ttm,
                    Some(other) => {
                        return Err(IdxError::Usage(format!(
                            "invalid period {other:?}; use annual, quarterly or ttm"
                        )))
                    }
                };
                ok(
                    SCHEMA_FINANCIALS,
                    self.provider.financials(&symbol, period).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_earnings",
        description = "Reported quarterly EPS versus analyst estimates with surprise %, fiscal-year and quarterly revenue/earnings, the next earnings date with consensus, and dividend dates for one IDX ticker. Returns the idx.earnings.v1 envelope."
    )]
    async fn earnings(
        &self,
        Parameters(a): Parameters<SymbolArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                ok(SCHEMA_EARNINGS, self.provider.earnings(&symbol).await?)
            }
            .await,
        )
    }

    #[tool(
        name = "idx_dividends",
        description = "Cash dividend and stock split history for one IDX ticker: every payment (ex-date, per-share amount as reported, split-adjusted), yearly totals with year-over-year change, trailing-twelve-month total and yield (TTM total / last price, computed by idx), the latest payment and the consecutive-years streak. Returns the idx.dividends.v1 envelope."
    )]
    async fn dividends(
        &self,
        Parameters(a): Parameters<SymbolArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                ok(
                    SCHEMA_DIVIDENDS,
                    service::dividends(self.provider.as_ref(), &symbol).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_brief",
        description = "Start here for one ticker: a single call that returns the quote, fundamentals with the valuation verdict, one-year technicals with the signal, returns versus IHSG, dividend facts (TTM yield, streak), the local >1% holder register (top five, foreign share) and company headlines with sentiment, plus key_points (plain-language one-liners) and sections_failed for anything a source could not provide. Each section has the same shape as the dedicated tool. Returns the idx.brief.v1 envelope."
    )]
    async fn brief(
        &self,
        Parameters(a): Parameters<SymbolArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                let list = universe::load(None).unwrap_or_default();
                ok(
                    SCHEMA_BRIEF,
                    service::brief(
                        self.provider.as_ref(),
                        &self.fetcher,
                        &list,
                        &self.ownership_db,
                        &symbol,
                    )
                    .await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_market",
        description = "Whole-market overview from one pass over ~950 IDX tickers: IHSG (^JKSE) quote and market state, breadth (advancing/declining/unchanged), total turnover, and top lists of gainers, losers (both above a market-cap floor), most traded by value and heavyweights by market cap. Call this first for 'how is the market today'. Returns the idx.market.v1 envelope."
    )]
    async fn market(
        &self,
        Parameters(a): Parameters<MarketArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let mut list = universe::load(None)?;
                universe::extend_from_ownership(&mut list, &self.ownership_db);
                let index = Symbol::parse("^JKSE")?;
                ok(
                    SCHEMA_MARKET,
                    service::market_overview(
                        self.provider.as_ref(),
                        &list,
                        &index,
                        a.top.unwrap_or(10) as usize,
                        a.min_market_cap.unwrap_or(1e12),
                    )
                    .await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_returns",
        description = "Price performance for one or more IDX tickers computed from five years of daily closes: total returns over 1w, 1m, 3m, 6m, YTD, 1y, 3y, 5y; CAGR 3y/5y; annualised 1y volatility; max drawdown (1y and whole window, with peak/trough dates); best and worst day; 52-week range and distance from the high; plus the benchmark index's returns and the ticker's excess over it (IHSG ^JKSE by default). Dividends are excluded. Returns the idx.returns.v1 envelope (array, order as requested)."
    )]
    async fn returns(
        &self,
        Parameters(a): Parameters<ReturnsArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbols = Symbol::parse_list([a.symbols.as_str()])?;
                let bench = match a.benchmark.as_deref().map(str::trim) {
                    None | Some("") => Some(Symbol::parse("^JKSE")?),
                    Some(s) if s.eq_ignore_ascii_case("none") => None,
                    Some(s) => Some(Symbol::parse(s)?),
                };
                ok(
                    SCHEMA_RETURNS,
                    service::returns(self.provider.as_ref(), &symbols, bench.as_ref()).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_fundamental",
        description = "Fundamentals report for one IDX ticker: valuation (P/E, forward P/E, PEG, P/B, P/S, EV/EBITDA), profitability (ROE, ROA, margins), balance sheet (debt/equity, current ratio, cash, debt), growth, dividend, per-share figures, analyst targets, and a plain-language verdict (undervalued | fair | overvalued | insufficient_data) with every rule that fired listed in reasons. Returns the idx.fundamentals.v1 envelope."
    )]
    async fn fundamental(
        &self,
        Parameters(a): Parameters<SymbolArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                ok(
                    SCHEMA_FUNDAMENTALS,
                    service::fundamentals(self.provider.as_ref(), &symbol).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_technical",
        description = "Technical analysis of one IDX ticker from daily candles: RSI(14), MACD(12,26,9), SMA/EMA 20/50/200, Bollinger(20,2), ATR(14), volume vs 20-day average, support/resistance from swing points, 52-week range, trend label and a buy | sell | neutral signal with reasons. Returns the idx.technical.v1 envelope."
    )]
    async fn technical(
        &self,
        Parameters(a): Parameters<TechnicalArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                let period = parse_opt(&a.period, Period::Y1)?;
                ok(
                    SCHEMA_TECHNICAL,
                    service::technical(self.provider.as_ref(), &symbol, period).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_compare",
        description = "Side-by-side fundamentals (same fields as idx_fundamental, verdict included) for two or more IDX tickers. Returns the idx.compare.v1 envelope: data is an array in the order requested; tickers that failed are listed in warnings."
    )]
    async fn compare(
        &self,
        Parameters(a): Parameters<SymbolsArg>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbols = Symbol::parse_list([a.symbols.as_str()])?;
                ok(
                    SCHEMA_COMPARE,
                    service::compare(self.provider.as_ref(), &symbols).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_screen",
        description = "Screens the IDX universe (~950 tickers) with a filter string such as \"pe<15,roe>15,sector=financial,mcap>10T\". Quote-level keys (pe fpe pb mcap price div eps volume chg) come from one batch quote; fundamentals keys (roe roa margin opmargin de growth epsgrowth peg ps sector industry) are fetched per company for the largest max_fundamentals survivors. Returns the idx.screen.v1 envelope with counts explaining how many tickers each stage kept."
    )]
    async fn screen(
        &self,
        Parameters(a): Parameters<ScreenArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let filters = crate::analysis::screen::parse_filters(&a.filter)?;
                let sort = match a.sort.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    None => crate::analysis::screen::Key::Mcap,
                    Some(s) => crate::analysis::screen::Key::parse(s).ok_or_else(|| {
                        IdxError::Usage(format!(
                            "unknown sort key {s:?}; use one of {}",
                            crate::analysis::screen::Key::ALL
                        ))
                    })?,
                };
                let mut list = universe::load(None)?;
                universe::extend_from_ownership(&mut list, &self.ownership_db);
                let opts = service::ScreenOptions {
                    filters,
                    sort,
                    descending: true,
                    limit: a.limit.unwrap_or(25) as usize,
                    max_fundamentals: a.max_fundamentals.unwrap_or(50) as usize,
                    with_fundamentals: false,
                };
                ok(
                    SCHEMA_SCREEN,
                    service::screen(self.provider.as_ref(), &list, &opts).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_news",
        description = "Latest Indonesian finance headlines from Kontan and CNBC Indonesia RSS feeds. With a symbol, headlines that mention the company (scope=company, matched by ticker or name) come first, followed by general market headlines (scope=market) up to limit; company_matches says how many company items exist. Each item carries a lexicon sentiment score and the matched terms. Returns the idx.news.v1 envelope."
    )]
    async fn news(
        &self,
        Parameters(a): Parameters<NewsArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let sym = a
                    .symbol
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(Symbol::parse)
                    .transpose()?;
                let list = universe::load(None).unwrap_or_default();
                let sources = a.sources.unwrap_or_default();
                ok(
                    SCHEMA_NEWS,
                    service::news(
                        &self.fetcher,
                        &list,
                        sym.as_ref(),
                        &sources,
                        a.limit.unwrap_or(20) as usize,
                        a.company_only.unwrap_or(false),
                    )
                    .await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_sentiment",
        description = "Aggregate lexicon sentiment (positive | negative | neutral | no_coverage, score -1..+1) over current headlines mentioning one IDX ticker, with counts, top terms and the strongest headlines. Indicative only. Returns the idx.sentiment.v1 envelope."
    )]
    async fn sentiment(
        &self,
        Parameters(a): Parameters<SentimentArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let sym = Symbol::parse(&a.symbol)?;
                let list = universe::load(None).unwrap_or_default();
                let sources = a.sources.unwrap_or_default();
                ok(
                    SCHEMA_SENTIMENT,
                    service::sentiment(&self.fetcher, &list, &sym, &sources, 10).await?,
                )
            }
            .await,
        )
    }

    #[tool(
        name = "idx_ownership_ticker",
        description = "Shareholders above 1% of one IDX ticker from KSEI's monthly register (local SQLite): holder name, investor type, local/foreign, shares, percent, sorted by percent. Returns the idx.ownership.holders.v1 envelope. Needs a prior `idx ownership import`."
    )]
    async fn ownership_ticker(
        &self,
        Parameters(a): Parameters<OwnershipTickerArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let symbol = Symbol::parse(&a.symbol)?;
                let as_of = parse_date(&a.as_of)?;
                let report = self.db()?.holders(symbol.as_str(), as_of)?;
                ok(SCHEMA_OWNERSHIP_HOLDERS, Sourced::fresh(report, "local-db"))
            }
            .await,
        )
    }

    #[tool(
        name = "idx_ownership_entity",
        description = "Every IDX position above 1% held by a person or institution whose name contains the query, from KSEI's monthly register (local SQLite). Spelling variants (PT/Tbk placement, punctuation) are grouped. Returns the idx.ownership.entity.v1 envelope."
    )]
    async fn ownership_entity(
        &self,
        Parameters(a): Parameters<OwnershipEntityArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                if a.name.trim().len() < 3 {
                    return Err(IdxError::Usage("name needs at least 3 characters".into()));
                }
                let as_of = parse_date(&a.as_of)?;
                let report = self.db()?.entity(&a.name, as_of)?;
                ok(SCHEMA_OWNERSHIP_ENTITY, Sourced::fresh(report, "local-db"))
            }
            .await,
        )
    }

    #[tool(
        name = "idx_ownership_changes",
        description = "Month-over-month changes in holders above 1% between two imported release dates: new holders, exits and position changes with percentage-point and share deltas, largest moves first. The summary block (counts, net percentage points for foreign vs local holders and per investor type, per-issuer flow, top rises and falls) always covers every match; limit/offset page the rows. Optional ticker filter. Returns the idx.ownership.changes.v1 envelope."
    )]
    async fn ownership_changes(
        &self,
        Parameters(a): Parameters<OwnershipChangesArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let from = parse_date(&Some(a.from.clone()))?.expect("checked");
                let to = parse_date(&Some(a.to.clone()))?.expect("checked");
                let symbol = a.symbol.as_deref().map(Symbol::parse).transpose()?;
                let limit = match a.limit {
                    Some(0) => None,
                    Some(n) => Some(n as usize),
                    None => Some(50),
                };
                let report = self.db()?.changes(
                    from,
                    to,
                    symbol.as_ref().map(Symbol::as_str),
                    a.min_pct_change.unwrap_or(0.0),
                    limit,
                    a.offset.unwrap_or(0) as usize,
                )?;
                ok(SCHEMA_OWNERSHIP_CHANGES, Sourced::fresh(report, "local-db"))
            }
            .await,
        )
    }

    #[tool(
        name = "idx_ownership_foreign",
        description = "Disclosed foreign (KSEI flag A) versus local (L) stake in the >1% register. With symbol: one issuer's foreign percent, holder counts and month-over-month change for every imported release (idx.ownership.foreign.v1). Without symbol: every issuer's change in foreign percent between two releases, largest moves first, with inflow/outflow counts (idx.ownership.foreign-flow.v1). Sums cover holders above 1% only. Needs a prior `idx ownership import`."
    )]
    async fn ownership_foreign(
        &self,
        Parameters(a): Parameters<OwnershipForeignArgs>,
    ) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let db = self.db()?;
                match a.symbol.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    Some(s) => {
                        let sym = Symbol::parse(s)?;
                        let report = db.foreign_trend(sym.as_str())?;
                        ok(SCHEMA_OWNERSHIP_FOREIGN, Sourced::fresh(report, "local-db"))
                    }
                    None => {
                        let limit = match a.limit {
                            Some(0) => None,
                            Some(n) => Some(n as usize),
                            None => Some(25),
                        };
                        let report =
                            db.foreign_flow(parse_date(&a.from)?, parse_date(&a.to)?, limit)?;
                        ok(
                            SCHEMA_OWNERSHIP_FOREIGN_FLOW,
                            Sourced::fresh(report, "local-db"),
                        )
                    }
                }
            }
            .await,
        )
    }

    #[tool(
        name = "idx_ownership_releases",
        description = "Lists the monthly KSEI register releases imported into the local database (as-of date, row counts, source). Use it to pick dates for idx_ownership_changes. Returns the idx.ownership.releases.v1 envelope."
    )]
    async fn ownership_releases(&self) -> std::result::Result<CallToolResult, McpError> {
        finish(
            async {
                let list = self.db()?.releases()?;
                let mut s = Sourced::fresh(list, "local-db");
                if s.data.is_empty() {
                    s.warnings.push(
                        "no ownership data imported yet; run `idx ownership import --latest`"
                            .into(),
                    );
                }
                ok(SCHEMA_OWNERSHIP_RELEASES, s)
            }
            .await,
        )
    }
}

#[tool_handler]
impl ServerHandler for IdxServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("idx", env!("CARGO_PKG_VERSION")))
            .with_protocol_version(ProtocolVersion::LATEST)
            .with_instructions(INSTRUCTIONS.to_string())
    }

    // Hand-written so the advertised schemas are the portable ones; the
    // `tool_handler` macro only fills in methods that are missing.
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: self.tools(),
            ..Default::default()
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools().into_iter().find(|t| t.name == name)
    }
}

/// Runs the server on stdin/stdout until the client disconnects.
pub async fn serve(
    provider: Arc<dyn Provider>,
    fetcher: Arc<Fetcher>,
    ownership_db: PathBuf,
) -> Result<()> {
    use rmcp::{transport::stdio, ServiceExt};
    let server = IdxServer::new(provider, fetcher, ownership_db);
    tracing::info!(tools = ?server.tool_names(), "idx mcp server starting on stdio");
    let running = server
        .serve(stdio())
        .await
        .map_err(|e| IdxError::Other(format!("mcp: {e}")))?;
    running
        .waiting()
        .await
        .map_err(|e| IdxError::Other(format!("mcp: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fundamentals, History, Quote};
    use async_trait::async_trait;

    struct Fake;

    #[async_trait]
    impl Provider for Fake {
        fn name(&self) -> &'static str {
            "fake"
        }
        async fn quotes(&self, s: &[Symbol]) -> Result<Sourced<Vec<Quote>>> {
            if s[0].as_str() == "ZZZZ" {
                return Err(IdxError::NotFound("ZZZZ".into()));
            }
            let mut q = Quote::empty(s[0].clone(), "fake");
            q.price = Some(5175.0);
            Ok(Sourced::fresh(vec![q], "fake"))
        }
        async fn history(&self, _: &Symbol, _: Period, _: Interval) -> Result<Sourced<History>> {
            Err(IdxError::OfflineMiss("nothing cached".into()))
        }
        async fn fundamentals(&self, s: &Symbol) -> Result<Sourced<Fundamentals>> {
            Ok(Sourced::fresh(
                Fundamentals::empty(s.clone(), "fake"),
                "fake",
            ))
        }
    }

    fn server() -> IdxServer {
        let dir = std::env::temp_dir().join(format!("idx-mcp-test-{}", std::process::id()));
        let fetcher = Arc::new(Fetcher::new(
            Arc::new(crate::http::FixtureBackend::new()),
            crate::cache::Cache::new(dir.join("cache")),
            crate::http::CacheMode::Offline,
        ));
        IdxServer::new(Arc::new(Fake), fetcher, dir.join("ownership.sqlite"))
    }

    fn text(r: &CallToolResult) -> String {
        r.content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect()
    }

    #[test]
    fn every_tool_is_listed_with_a_schema() {
        let s = server();
        let mut names = s.tool_names();
        names.sort();
        assert_eq!(
            names,
            [
                "idx_brief",
                "idx_compare",
                "idx_dividends",
                "idx_earnings",
                "idx_financials",
                "idx_fundamental",
                "idx_history",
                "idx_market",
                "idx_news",
                "idx_ownership_changes",
                "idx_ownership_entity",
                "idx_ownership_foreign",
                "idx_ownership_releases",
                "idx_ownership_ticker",
                "idx_profile",
                "idx_quote",
                "idx_returns",
                "idx_screen",
                "idx_sentiment",
                "idx_technical",
            ]
        );
        for t in s.tools() {
            let d = t.description.as_deref().unwrap_or("");
            assert!(d.len() > 60, "{} needs a real description", t.name);
            assert!(
                t.input_schema.get("properties").is_some(),
                "{} has no typed input",
                t.name
            );
            let text = serde_json::to_string(&*t.input_schema).unwrap();
            assert!(!text.contains("\"$schema\""), "{}: $schema key", t.name);
            assert!(!text.contains("\"format\""), "{}: format keyword", t.name);
            assert!(
                !text.contains("\"null\""),
                "{}: nullable type array breaks Gemini/OpenAI harnesses",
                t.name
            );
        }
    }

    #[tokio::test]
    async fn quote_tool_returns_the_cli_envelope() {
        let s = server();
        let r = s
            .quote(Parameters(SymbolsArg {
                symbols: "bbca".into(),
            }))
            .await
            .unwrap();
        assert_ne!(r.is_error, Some(true));
        let v: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        assert_eq!(v["schema"], "idx.quotes.v1");
        assert_eq!(v["data"][0]["symbol"], "BBCA");
        assert_eq!(v["data"][0]["price"], 5175.0);
    }

    #[tokio::test]
    async fn failures_are_tool_errors_with_the_cli_error_shape() {
        let s = server();
        let r = s
            .quote(Parameters(SymbolsArg {
                symbols: "ZZZZ".into(),
            }))
            .await
            .unwrap();
        assert_eq!(r.is_error, Some(true));
        let v: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        assert_eq!(v["error"]["code"], "not_found");
        assert_eq!(v["exit_code"], 4);

        let r = s
            .history(Parameters(HistoryArgs {
                symbol: "BBCA".into(),
                period: Some("7d".into()),
                interval: None,
                limit: None,
            }))
            .await
            .unwrap();
        assert_eq!(r.is_error, Some(true));
        assert!(text(&r).contains("usage"));

        let r = s
            .history(Parameters(HistoryArgs {
                symbol: "BBCA".into(),
                period: None,
                interval: None,
                limit: None,
            }))
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        assert_eq!(v["error"]["code"], "offline_miss");
    }

    #[tokio::test]
    async fn fundamental_tool_adds_a_verdict() {
        let s = server();
        let r = s
            .fundamental(Parameters(SymbolArg {
                symbol: "BBCA".into(),
            }))
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&text(&r)).unwrap();
        assert_eq!(v["schema"], "idx.fundamentals.v1");
        assert_eq!(v["data"]["verdict"]["label"], "insufficient_data");
    }
}
