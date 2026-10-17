# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- `broker`: the IPOT summary ranks its top 10 buyers and sellers by lots, not
  by value as the help, the docs and the JSON Schema said. Of two brokers
  the one with more lots comes first even when it traded less value.

### Added

- Web terminal: `quote` with a single symbol draws a price chart under the
  table, with ranges 1D, 5D, 1M, 6M, YTD, 1Y and 5Y, a volume strip, the
  previous close on 1D and a crosshair readout of each candle (pointer, touch
  or arrow keys).

## [0.5.0] - 2026-10-17

### Added

- `idx broker <SYMBOLS>`: which brokers bought and sold a ticker. The top 10
  buyers and top 10 sellers by value from Indo Premier's (IPOT) public broker
  summary, each with the broker code, name, class (local, foreign, BUMN),
  lots, value and average price, plus the range's total value, lots, average
  price and foreign net value. `--date`, `--from/--to`, `--investor
  all|foreign|domestic`, `--board all|regular|cash|negotiated`, up to 10
  tickers per call; without dates each ticker gets its latest session with
  trades. JSON schema `idx.broker.v1`; `-o csv` writes one row per broker.
  The source rounds figures of a million and more to one decimal, so those
  summaries carry `rounded: true`.
- `data/brokers.csv`: IDX's exchange-member list (89 brokers), bundled to
  name the codes.
- MCP tool `idx_broker` (21 tools now), `/api/broker` and a `broker` command
  in the web terminal.
- Web: Open Graph / Twitter share card, canonical URLs, JSON-LD,
  `robots.txt`, `sitemap.xml` and an apple-touch icon on
  ihsg.midoriresearch.com; API responses are `noindex`.

### Changed

- Package metadata and the licence credit Midori Research
  (desk@midoriresearch.com) instead of a personal name and address.

## [0.4.0] - 2026-10-17

### Added

- Published as **`ihsg-cli`** on crates.io (`cargo install ihsg-cli`) and npm
  (`npm i -g ihsg-cli`, `npx ihsg-cli`); the command is still `idx`. The
  release workflow publishes both on every tag once `CRATES_PUBLISH` /
  `NPM_PUBLISH` are enabled (token secret or Trusted Publishing).
- Ownership data pipeline: `.github/workflows/ownership.yml` and
  `scripts/ownership_mirror.py` mirror every IDX "Pemegang Saham di Atas 1%"
  workbook to the `ownership-data` prerelease, with `index.json` (original
  URL, SHA-256, counts) and parsed JSON parts. idx.co.id blocks cloud
  networks, so the job runs on a self-hosted runner (`OWNERSHIP_RUNNER`); on
  GitHub's runners it only alarms when the mirror is 45 days stale. The
  mirror holds every register from February to September 2026.
- The web terminal loads the register by itself from that release (one
  verified part per minute, newest six months, D1 write budget); new
  `/api/ownership/status`. The manual `web/scripts/ownership-sync.mjs` is gone.
- `idx ownership import --latest|--all` and `discover` fall back to the GitHub
  mirror when idx.co.id refuses the request, checking each file's SHA-256;
  `IDX_OWNERSHIP_MIRROR` overrides or disables it, `IDX_OWNERSHIP_SOURCE`
  forces either side. `file://` URLs are accepted.
  `idx.ownership.discover.v1` entries gain `origin`, `sha256`, `source_url`.

### Changed

- Logo: the wordmark is now "IHSG" with the tagline "Indonesian market data
  for people and agents".
- Web ownership rows now carry nationality, domicile and scripless/scrip
  share counts like the CLI.

## [0.3.0] - 2026-10-17

### Added

- `idx mcp setup <client>`: the registration command or config snippet for
  25 agent harnesses (Claude Code, Claude Desktop, OpenAI Codex, Meta Muse
  Code, Google Antigravity, Gemini CLI, GitHub Copilot CLI, VS Code, Cursor,
  Windsurf, Kiro, JetBrains, Zed, Cline/Roo/Kilo, Continue, OpenCode, Goose,
  Warp, Factory Droid, Mistral Vibe, Qwen Code, Crush, Trae, Augment, and the
  remote-only products), with the binary's absolute path filled in; `--npx`
  and `--http URL --token T` variants; `-o json` for scripts.
- `idx mcp --http ADDR [--token T] [--allowed-host H]`: the same 20 tools
  over MCP streamable HTTP at `/mcp` for ChatGPT, Claude.ai connectors,
  Perplexity, Devin, Manus and other URL-only clients. Bearer auth, Host
  allow-list, `/healthz`.
- README and docs/mcp.md: install guide per agent.

## [0.2.0] - 2026-10-17

### Added

- `stocks brief SYMBOL` and MCP `idx_brief`: quote, verdict, signal, returns
  vs IHSG, dividend facts, top holders and headlines in one call, with
  plain-language `key_points` and a `sections_failed` list.
- `stocks dividends SYMBOL` / `idx_dividends`: Yahoo chart events
  (`events=div,splits`), yearly totals, TTM total and yield, payout streak.
- `stocks returns SYMBOLS` / `idx_returns`: 1w..5y and YTD returns, CAGR,
  volatility, max drawdown, best/worst day, 52-week position, and excess over
  a benchmark index (`^JKSE` by default). Index codes now parse everywhere.
- `market overview` / `idx_market`: IHSG, breadth, turnover, gainers, losers,
  most traded, heavyweights.
- `ownership foreign [SYMBOL]` / `idx_ownership_foreign`: disclosed foreign vs
  local stake per issuer over time, or every issuer's change between months.
- `ownership changes --offset` paging and a `summary` block (counts, foreign/
  local and investor-type net, issuer flow, top rises and falls) over every
  match; rows carry `local_foreign`.
- `-o csv` for every command.
- `idx watch add|rm|list|clear|returns` and `idx watch` for a local watchlist.
- Web terminal and landing page at https://ihsg.midoriresearch.com with a
  minute-refreshed D1 snapshot API (`web/`), now serving the new endpoints
  (`dividends`, `returns`, `brief`, `ownership/foreign`) too.

### Changed

- `stocks news SYMBOL` lists the company's headlines first and then fills the
  limit with market headlines (`scope` field, `company_matches`);
  `--company-only` restores the strict filter. Sentiment uses company items only.
- MCP tool input schemas are rewritten into the subset every harness accepts
  (no `$schema`, no `format`, no nullable type arrays), so the tools register
  in Gemini and OpenAI-strict clients as well as Claude.
- README logo respects GitHub dark mode and no longer clips its tagline.

### Fixed

- npm wrapper: npm 12 blocks dependency install scripts by default, so the
  package arrived without a binary. The launcher now downloads and verifies
  the binary on first run when postinstall did not.

## [0.1.0] - 2026-10-17

### Added

- `stocks quote <SYMBOLS>`: delayed quotes with price, change, OHLC, volume,
  market cap, P/E, dividend yield and more, from Yahoo Finance. Falls back to
  the chart endpoint (reduced fields) when the quote endpoint is unavailable.
- `stocks history <SYMBOL> --period --interval --limit`: OHLCV candles with a
  summary line.
- `-o json`: stable envelope (`idx.quotes.v1`, `idx.history.v1`) with
  provenance (`provider`, `fetched_at`, `cached`, `stale`, `warnings`).
- `idx schema <name>`: JSON Schema for every JSON shape.
- Local response cache with per-endpoint TTL, `--offline`, `--no-cache`, and
  `idx cache stats|clear|path`.
- `stocks screen --filter "pe<15,roe>15,sector=financial"`: screener over the
  bundled IDX universe (955 issuers, `data/universe.csv`). Quote-level keys
  come from batched Yahoo quotes; fundamentals keys from per-company calls
  capped at `--max-fundamentals` (largest first). Reports how many tickers
  each stage kept.
- `stocks news [SYMBOL]` and `stocks sentiment SYMBOL`: Kontan and CNBC
  Indonesia RSS feeds, ticker/company-name matching, transparent lexicon
  sentiment (terms are listed). Bisnis.com is opt-in because it blocks
  non-browser clients.
- MCP tools `idx_screen`, `idx_news`, `idx_sentiment`.
- `ownership import|ticker|entity|changes|releases|discover|path`: KSEI's
  monthly "Pemegang Saham di Atas 1%" register parsed from the IDX XLSX (or
  the older PDF text layer) into local SQLite, with entity-name normalisation
  so spelling variants of one holder group together. Downloads try a
  browser-shaped request, then `curl-impersonate`, else explain the manual
  path (idx.co.id is behind Cloudflare).
- MCP tools `idx_ownership_ticker`, `idx_ownership_entity`,
  `idx_ownership_changes`, `idx_ownership_releases`.
- `idx mcp`: MCP server over stdio (rmcp) exposing `idx_quote`, `idx_history`,
  `idx_profile`, `idx_financials`, `idx_earnings`, `idx_fundamental`,
  `idx_technical` and `idx_compare`, each returning the `-o json` envelope.
- `stocks profile`, `stocks financials` (annual / quarterly / TTM), `stocks
  earnings`, `stocks fundamental` (ratios plus a transparent rule-based
  verdict), `stocks technical` (RSI, MACD, SMA/EMA, Bollinger, ATR, levels,
  trend, signal) and `stocks compare`.
- MSN Money provider (`--provider msn`): quotes, profile, fundamentals and
  analyst targets. `auto` now tries Yahoo then MSN and merges fundamentals
  field by field, never averaging.
- Yahoo quoteSummary and fundamentals-timeseries endpoints.
- `web/`: Cloudflare Worker serving the landing page at
  https://ihsg.midoriresearch.com, reachable on the custom domain only.
- Config file with `idx config init|set|get|path`; precedence flags > env > file > defaults.
- Shell completions for bash, zsh, fish, PowerShell and elvish.
- Stable exit codes (0 ok, 2 usage, 3 network/provider, 4 not found,
  5 offline cache miss, 6 config).
