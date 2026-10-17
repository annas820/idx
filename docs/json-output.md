# JSON output

Add `-o json` (or `IDX_OUTPUT=json`, or `output = "json"` in the config file)
and every command prints one JSON document to stdout. Errors go to stderr as
JSON too. Nothing else is ever written to stdout in JSON mode, so
`idx -o json ... | jq` is safe.

Machine-readable schemas: `idx schema <name>` with `quotes`, `history`,
`profile`, `fundamentals`, `compare`, `financials`, `earnings`, `dividends`,
`returns`, `market`, `brief`, `technical`, `screen`, `news`, `sentiment`,
`ownership-holders`, `ownership-entity`, `ownership-changes`,
`ownership-foreign`, `ownership-foreign-flow`, `ownership-releases`,
`ownership-import`, `ownership-discover`, `watchlist`, `error`, `quote`,
`candle`.

`-o csv` prints the payload's main list (candles, rows, items, holders,
changes, dividends, ...) as comma-separated rows with a header; nested objects
become dotted columns (`valuation.pe_trailing`), arrays of scalars join with
`|`, and a payload with no list becomes one row. Warnings still go to stderr.

## Envelope

Every successful command wraps its payload:

```json
{
  "schema": "idx.quotes.v1",
  "provider": "yahoo",
  "fetched_at": "2026-10-16T17:45:07Z",
  "cached": false,
  "stale": false,
  "warnings": ["no data for ZZZZ"],
  "data": ...
}
```

| field | type | meaning |
| --- | --- | --- |
| `schema` | string | Payload shape and version. Changes only when a field is renamed or removed. |
| `provider` | string | Source that produced the data (`yahoo`, later `msn`, ...). |
| `fetched_at` | RFC 3339 UTC | When the underlying response left the network. For cached responses this is the original fetch time. |
| `cached` | bool | Served from the local cache. |
| `stale` | bool | The cached copy was older than its TTL and was used anyway (`--offline`, or the network failed). |
| `warnings` | string[] | Non-fatal notes. Omitted when empty. |
| `data` | object or array | The payload, see below. |

## `idx.quotes.v1` (`stocks quote`)

`data` is an array of quote objects, one per symbol the source knew. Symbols
it did not know appear in `warnings`; if none were known the command fails
with exit code 4.

| field | type | notes |
| --- | --- | --- |
| `symbol` | string | IDX ticker without suffix, e.g. `BBCA` |
| `name` | string? | Company name |
| `currency` | string? | `IDR` |
| `price` | number? | Last trade |
| `change`, `change_pct` | number? | Versus previous close; `change_pct` is in percent (`-14.81`) |
| `previous_close`, `open`, `day_high`, `day_low` | number? | |
| `volume`, `avg_volume_3m` | integer? | Shares |
| `market_cap` | number? | In `currency` |
| `pe_trailing`, `pe_forward`, `eps_ttm`, `price_to_book` | number? | |
| `dividend_yield_pct` | number? | Percent, e.g. `6.27` |
| `week52_high`, `week52_low`, `sma50`, `sma200` | number? | |
| `shares_outstanding` | integer? | |
| `market_state` | string? | `REGULAR`, `CLOSED`, ... |
| `market_time` | RFC 3339? | Last trade time in WIB (`+07:00`) |
| `source` | string | Endpoint that produced the row: `yahoo:quote` (full) or `yahoo:chart` (reduced fallback) |

`?` means the value is `null` when the source did not return it. Nothing is
estimated. When `source` is `yahoo:chart`, expect `market_cap`, P/E, dividend
and average-volume fields to be `null`.

## `idx.history.v1` (`stocks history`)

```json
{
  "symbol": "TLKM",
  "name": "...",
  "currency": "IDR",
  "period": "1y",
  "interval": "1wk",
  "candles": [
    { "time": "2025-10-13T09:00:00+07:00", "open": 2900.0, "high": 2960.0, "low": 2850.0,
      "close": 2880.0, "adj_close": 2804.6, "volume": 512345600 }
  ],
  "summary": {
    "start": "2025-10-13T09:00:00+07:00", "end": "2026-10-16T09:00:00+07:00",
    "first_close": 2880.0, "last_close": 1900.0, "change": -980.0, "change_pct": -34.03,
    "high": 3990.0, "low": 1900.0, "avg_volume": 578600000, "candles": 54
  }
}
```

- `candles` is oldest first and is **never** truncated by `--limit` (that flag
  only affects the table).
- Bars with a missing open/high/low/close (holidays, or the current session
  before its first trade) are dropped.
- `adj_close` is present when the source supplies split/dividend adjusted
  closes.
- `summary` is `null` when there are no candles.

## `idx.screen.v1` (`stocks screen`)

`data.rows[]` carry the quote-level fields (`price`, `change_pct`,
`market_cap`, `pe_trailing`, `pe_forward`, `price_to_book`,
`dividend_yield_pct`, `eps_ttm`, `avg_volume_3m`, 52-week range) and, when
`fundamentals_checked` is true, `sector`, `industry`, `roe_pct`, `roa_pct`,
`net_margin_pct`, `operating_margin_pct`, `debt_to_equity`,
`revenue_growth_pct`, `earnings_growth_pct`, `peg`, `price_to_sales`.
`data.universe`, `quoted`, `matched_quote_level`, `fundamentals_checked`,
`fundamentals_capped` and `total_matches` describe the funnel, so a reader can
tell a genuine "no match" from a capped fundamentals pass.

## `idx.news.v1` (`stocks news`) and `idx.sentiment.v1` (`stocks sentiment`)

News items: `title`, `url`, `source` (`kontan/investasi`,
`cnbcindonesia/market`, ...), `published` (WIB), `summary`, `matched_by`
(`ticker` | `name` | `all`), `scope` (`company` when the item mentions the
requested ticker, else `market`), `sentiment` (-1..+1), `positive_terms`,
`negative_terms`. With a symbol, `items` lists company headlines first and
then market headlines up to the limit; `company_matches` counts the company
headlines before the limit and `company_only` records whether market context
was suppressed. The sentiment report (company headlines only) adds `label`
(`positive` | `negative` | `neutral` | `no_coverage`), `score`, counts, the
covered period, top terms and the strongest `headlines`, plus `method`
describing the lexicon rule.

## `idx.dividends.v1` (`stocks dividends`)

`dividends[]` (`date` = ex-date, `amount` per share as reported, split-adjusted
on Yahoo) and `splits[]` (`date`, `numerator`, `denominator`, `ratio`) oldest
first, with `price` and `price_date` from the same response. Computed by idx
and documented as such: `years[]` (`year`, `payments`, `total`, `change_pct`),
`last`, `ttm_total`, `ttm_payments`, `ttm_yield_pct` (= TTM total / price),
`consecutive_years`, `as_of`.

## `idx.broker.v1` (`broker`)

Array, one summary per requested ticker, as Indo Premier (IPOT) publishes it:
`symbol`, `name` (bundled universe), `from` and `to` (the range the source
applied, WIB dates), `investor` (`all` | `foreign` | `domestic`), `board`
(`all` | `regular` | `cash` | `negotiated`), `buyers[]` and `sellers[]` (top 10
ranked by lots, most first: `rank`, `code`, `name` from the bundled IDX member
list or `null`, `kind` = `local` | `foreign` | `state_owned` or `null`, `lots`
(1 lot = 100 shares), `value` in IDR, `average_price`), and the range totals
`total_value`, `total_lots`, `foreign_net_value` (foreign buy minus sell),
`average_price` (`null` when nothing traded). The source abbreviates figures
of a million and more to one decimal (`1.6 M`); those arrive as plain numbers
at that precision and the summary carries `rounded: true`. Nothing is netted
per broker or estimated. A range without trades has empty lists and a warning.
With `-o csv` the rows are one broker per side: `symbol, from, to, investor,
board, side (buy|sell), rank, code, name, kind, lots, value, average_price`.

## `idx.returns.v1` (`stocks returns`)

Array, one `Performance` per requested ticker: `as_of`, `price`, `start`,
`candles_used`, `returns` (`1w 1m 3m 6m ytd 1y 3y 5y`, percent, `null` when
the history is too short), `cagr_3y_pct`, `cagr_5y_pct`, `volatility_1y_pct`
(annualised stdev of daily log returns), `max_drawdown_1y` and
`max_drawdown_all` (`pct`, `peak_date`, `trough_date`, `recovered`),
`best_day_1y`, `worst_day_1y`, `high_52w`, `low_52w`, `pct_from_52w_high`,
`pct_from_52w_low`, `days_since_52w_high`, and `benchmark` (`symbol`, `name`,
`returns`, `excess` in percentage points) or `null`. Everything is computed
from five years of daily closes; dividends are excluded.

## `idx.market.v1` (`market overview`)

`as_of`, `market_state`, `index` (a `MarketRow` for `^JKSE`), `universe`,
`quoted`, `advancing`, `declining`, `unchanged`, `turnover`,
`liquid_min_market_cap`, and the lists `gainers`, `losers`, `most_active`,
`heavyweights` of `MarketRow` (`symbol`, `name`, `price`, `change`,
`change_pct`, `volume`, `value`, `market_cap`). The web terminal serves the
same shape plus `index_tape` and `refreshed_at`.

## `idx.brief.v1` (`stocks brief`)

`symbol`, `name`, `generated_at`, `key_points[]` (plain-language lines), then
one section per topic, each either `null` or the dedicated command's object:
`quote` (one `Quote`), `fundamentals` (`idx.fundamentals.v1` data),
`technical`, `returns` (one `Performance`), `dividends` (`ttm_total`,
`ttm_payments`, `ttm_yield_pct`, `consecutive_years`, `last`,
`payments_on_record`), `ownership` (`as_of`, `listed_pct`, `foreign_pct`,
`holders_listed`, `top_holders[]`), `news` (`company_matches`, `sentiment`,
`score`, `headlines[]`), and `sections_failed[]` naming the `null` sections,
with the reason in the envelope's `warnings`.

## `idx.watchlist.v1` (`watch add|rm|list|clear`)

`path` and `symbols[]`. `idx watch` itself returns `idx.quotes.v1` and
`idx watch returns` returns `idx.returns.v1`.

## `idx.profile.v1` (`stocks profile`)

`data` is one object: `symbol`, `name`, `sector`, `industry`, `summary`,
`employees`, `founded_year`, `website`, `city`, `country`, `exchange`,
`currency`, `market_cap`, `shares_outstanding`, `directors[]` (`name`,
`title`; populated by MSN, empty from Yahoo) and `source`.

## `idx.fundamentals.v1` (`stocks fundamental`)

`data` is one object grouped by theme. Percent fields end in `_pct` and are
already percentages (`21.8` = 21.8 %). Ratios are plain multiples
(`debt_to_equity: 0.6` = 60 % of equity).

```json
{
  "symbol": "BBCA", "name": "PT Bank Central Asia Tbk", "currency": "IDR",
  "sector": "Financial Services", "industry": "Banks - Regional",
  "price": 5175.0, "beta": -0.017, "week52_high": 8750.0, "week52_low": 4820.0,
  "valuation": { "market_cap": 6.357e14, "enterprise_value": 6.099e14, "pe_trailing": 12.87,
                 "pe_forward": 10.73, "peg": 1.52, "price_to_book": 2.35, "price_to_sales": 5.82,
                 "ev_to_ebitda": null, "ev_to_revenue": 5.58 },
  "profitability": { "roe_pct": 21.82, "roa_pct": 3.67, "gross_margin_pct": null,
                     "operating_margin_pct": 66.75, "net_margin_pct": 53.12 },
  "health": { "debt_to_equity": null, "current_ratio": null, "quick_ratio": null,
              "total_debt": 5.2e13, "total_cash": 7.8e13 },
  "growth": { "revenue_growth_pct": 2.5, "earnings_growth_pct": 0.4,
              "revenue_cagr_5y_pct": 8.63, "eps_growth_3y_pct": 12.21 },
  "dividend": { "yield_pct": 6.27, "payout_ratio_pct": 75.42, "ex_dividend_date": "2026-08-31" },
  "per_share": { "eps_ttm": 402.07, "eps_forward": 482.48, "book_value": 2201.5, "revenue": 888.2 },
  "analyst": { "recommendation": "strong_buy", "target_mean": 8197.5, "target_high": 10100.0,
               "target_low": 5900.0, "analysts": 24 },
  "fiscal_year_end": "2025-12-31", "most_recent_quarter": "2026-06-30",
  "verdict": { "label": "fair", "score": 2, "reasons": ["trailing P/E 12.9 is moderate (10 to 15)", "..."],
               "inputs_available": 7 },
  "sources": ["yahoo:quoteSummary", "msn:equities", "msn:quotes"]
}
```

- `auto` asks Yahoo first and fills remaining `null`s from MSN; `sources`
  lists who contributed. Values are never averaged across sources.
- Yahoo reports gross and EBITDA margins as `0` for banks; that is published
  as `null` because it means "not applicable", not 0 %.
- `verdict.label` is `undervalued` | `fair` | `overvalued` |
  `insufficient_data` (fewer than three rule inputs). Every rule that fired is
  in `reasons`. It is a heuristic, not advice.

## `idx.compare.v1` (`stocks compare`)

`data` is an array of `idx.fundamentals.v1` objects in the order requested.
Symbols that failed are listed in `warnings`.

## `idx.financials.v1` (`stocks financials`)

```json
{
  "symbol": "BBCA", "currency": "IDR", "period": "annual",
  "periods": [
    { "end_date": "2025-12-31", "period": "annual",
      "income": { "total_revenue": 1.14e14, "gross_profit": null, "operating_income": null,
                  "ebitda": null, "ebit": null, "interest_income": 9.8e13, "interest_expense": 1.0e13,
                  "net_interest_income": 8.8e13, "pretax_income": 7.1e13, "tax_provision": 1.4e13,
                  "net_income": 5.75e13, "net_income_common": 5.75e13, "eps_basic": 467.0, "eps_diluted": 467.0 },
      "balance": { "total_assets": 1.59e15, "total_liabilities": 1.31e15, "stockholders_equity": 2.81e14,
                   "cash_and_equivalents": 9.1e13, "total_debt": 2.4e12, "net_debt": null, "shares_outstanding": 1.23e11 },
      "cash_flow": { "operating": 7.75e13, "investing": -3.4e13, "financing": -4.2e13,
                     "capital_expenditure": -2.45e12, "free_cash_flow": 7.5e13, "dividends_paid": -3.76e13 } }
  ],
  "source": "yahoo:timeseries"
}
```

- `period` is `annual` (about four fiscal years), `quarterly` (about five
  quarters) or `ttm` (trailing twelve months; `balance` fields are `null`).
- `periods` is oldest first. Amounts are in `currency` units as reported;
  outflows are negative.
- Banks have no EBITDA, gross profit or operating income series at the source,
  so those are `null`.

## `idx.earnings.v1` (`stocks earnings`)

`data`: `quarters[]` (`period` like `3Q2025`, `end_date`, `reported_date`,
`eps_actual`, `eps_estimate`, `surprise_pct`), `yearly[]` and `quarterly[]`
(`period`, `revenue`, `earnings`, `profit_margin_pct`), `next` (`date`,
`date_is_estimate`, `eps_avg`/`eps_low`/`eps_high`, `eps_growth_pct`,
`revenue_avg`/`revenue_low`/`revenue_high`, `analysts`), `ex_dividend_date`,
`dividend_date`, `source`.

## `idx.technical.v1` (`stocks technical`)

Computed locally from daily candles (`provider` names where the candles came
from). `data`: `as_of`, `period`, `candles_used`, `price`,
`change_pct_1d|1w|1m|3m`, `high_52w`, `low_52w`,
`moving_averages` (`sma20|50|200`, `ema20|50|200`), `rsi14`,
`macd` (`line`, `signal`, `histogram`), `bollinger` (`upper`, `middle`,
`lower`, `percent_b`, `bandwidth_pct`), `atr14`, `volume` (`last`, `avg20`,
`ratio`), `levels` (`support[]`, `resistance[]`, nearest first),
`trend` (`uptrend` | `downtrend` | `sideways` | `unknown`) and
`signal` (`label` `buy` | `sell` | `neutral`, `score`, `reasons[]`).
Indicators that need more history than was available are `null`.

## Errors

Written to stderr, exit code non-zero:

```json
{"error":{"code":"not_found","message":"not found: no quotes for ZZZZ"},"exit_code":4}
```

| exit | `code` | when |
| --- | --- | --- |
| 1 | `internal`, `io` | unexpected failure |
| 2 | `usage` | bad flags or symbol |
| 3 | `network`, `provider` | network failure or the source returned an error |
| 4 | `not_found` | symbol or record does not exist |
| 5 | `offline_miss` | `--offline` and nothing cached |
| 6 | `config`, `unsupported` | bad config file or env value; or the selected provider cannot serve this command (`--provider auto` can) |

## Compatibility promise

- Adding an optional field to a payload does not bump the `schema` version.
- Renaming, removing or changing the type/unit of a field does, and the old
  version is kept for at least one minor release behind a flag.
- Numbers are plain JSON numbers (no strings), timestamps are RFC 3339.
