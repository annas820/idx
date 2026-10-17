# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
