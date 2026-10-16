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
- Config file with `idx config init|set|get|path`; precedence flags > env > file > defaults.
- Shell completions for bash, zsh, fish, PowerShell and elvish.
- Stable exit codes (0 ok, 2 usage, 3 network/provider, 4 not found,
  5 offline cache miss, 6 config).
