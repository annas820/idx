# MCP server

`idx mcp` serves the Model Context Protocol over stdin/stdout. It is the same
binary and the same code path as the CLI: every tool calls the provider layer
and returns the envelope documented in [json-output.md](json-output.md).

## Register

Claude Code:

```sh
claude mcp add idx -- idx mcp
# or, scoped to one project and with a provider pinned:
claude mcp add --scope project idx -e IDX_PROVIDER=yahoo -- idx mcp
```

Generic JSON config (Cursor, Windsurf, Claude Desktop, ...):

```json
{ "mcpServers": { "idx": { "command": "idx", "args": ["mcp"] } } }
```

All `IDX_*` environment variables and the config file apply, so `--offline`
behaviour, cache location, TTLs and provider choice can be set per client.

## Tools

| tool | arguments | returns |
| --- | --- | --- |
| `idx_brief` | `symbol` | `idx.brief.v1`: quote, fundamentals + verdict, technicals + signal, returns vs IHSG, dividend facts, top holders, headlines + sentiment, `key_points`, `sections_failed`. Start here. |
| `idx_market` | `top?`, `min_market_cap?` | `idx.market.v1`: IHSG quote, breadth, turnover, gainers, losers, most traded, heavyweights |
| `idx_quote` | `symbols` ("BBCA,BBRI"; `^JKSE` is IHSG) | `idx.quotes.v1` |
| `idx_history` | `symbol`, `period?`, `interval?`, `limit?` (default 60 most recent candles, 0 = all) | `idx.history.v1` |
| `idx_profile` | `symbol` | `idx.profile.v1` |
| `idx_financials` | `symbol`, `period?` (`annual` \| `quarterly` \| `ttm`) | `idx.financials.v1` |
| `idx_earnings` | `symbol` | `idx.earnings.v1` |
| `idx_dividends` | `symbol` | `idx.dividends.v1`: payments, splits, yearly totals, TTM total/yield, streak |
| `idx_returns` | `symbols`, `benchmark?` (`^JKSE` default, `none`) | `idx.returns.v1`: windows, CAGR, volatility, drawdowns, 52-week, excess over the benchmark |
| `idx_fundamental` | `symbol` | `idx.fundamentals.v1` (verdict included) |
| `idx_technical` | `symbol`, `period?` | `idx.technical.v1` |
| `idx_compare` | `symbols` | `idx.compare.v1` |
| `idx_screen` | `filter`, `sort?`, `limit?`, `max_fundamentals?` | `idx.screen.v1` |
| `idx_news` | `symbol?`, `sources?`, `limit?`, `company_only?` | `idx.news.v1`: company items first (`scope=company`), then market context |
| `idx_sentiment` | `symbol`, `sources?` | `idx.sentiment.v1` |
| `idx_ownership_ticker` | `symbol`, `as_of?` | `idx.ownership.holders.v1` |
| `idx_ownership_entity` | `name`, `as_of?` | `idx.ownership.entity.v1` |
| `idx_ownership_changes` | `from`, `to`, `symbol?`, `min_pct_change?`, `limit?`, `offset?` | `idx.ownership.changes.v1`: `summary` over every match, one page of rows |
| `idx_ownership_foreign` | `symbol?`, `from?`, `to?`, `limit?` | `idx.ownership.foreign.v1` (one issuer over time) or `idx.ownership.foreign-flow.v1` (every issuer between two months) |
| `idx_ownership_releases` | none | `idx.ownership.releases.v1` |

Paging: `idx_ownership_changes` returns `total_matches` and `offset`; ask for
the next page with `offset = offset + limit` until `offset + rows >= total_matches`.
The `summary` block never depends on the page, so one call is enough to
describe a month.

## Harness compatibility

Input schemas are rewritten before they are advertised: no `$schema`, no
`format` (`uint32`, `double`), and optional fields are typed `"string"` not
`["string", "null"]` (they are simply absent from `required`). Claude accepts
either form; Gemini function declarations and OpenAI strict tool mode reject
the latter, and a unit test plus the stdio test keep it that way.

Tool results are a single `text` content block holding the pretty-printed
envelope. Failures set `isError: true` and the text is the CLI error object:

```json
{"error":{"code":"not_found","message":"not found: ZZZZ: Quote not found for symbol: ZZZZ.JK"},"exit_code":4}
```

`code` values and their meaning are listed in json-output.md.

## Behaviour notes

- Logs go to stderr (`-v` raises the level, `IDX_LOG` overrides). stdout
  carries only protocol frames.
- Responses are cached exactly like the CLI, so an agent calling
  `idx_quote` twice within the quote TTL hits the network once.
- The server exits cleanly when the client closes stdin.
- Verdicts and signals are heuristics; the tool descriptions and the server
  instructions say so, and so should any agent that repeats them.

## Verifying

```sh
claude mcp get idx            # health check: "Connected"
cargo test --test mcp         # JSON-RPC over stdio without network
```
