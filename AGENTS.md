# AGENTS.md

Guidance for AI agents (and humans) working on this repository. `CLAUDE.md`
imports this file.

## What this is

`idx` is a Rust CLI, MCP server and web terminal for Indonesian stock market
(IDX / IHSG) analysis. One binary serves three audiences:

- **People** run `idx stocks quote BBCA` and read a table.
- **Scripts and agents** run `idx -o json stocks quote BBCA` and parse a stable envelope.
- **MCP clients** (Claude Code, Cursor, ...) run `idx mcp` and call the same
  capabilities as typed tools.

Data comes only from free public endpoints, with no API keys. Every response is
cached locally. `--offline` serves stale cache instead of failing.

## Layout

```
src/
  main.rs            thin binary: parse args, init tracing, map errors to exit codes
  lib.rs             module map
  cli/               clap definitions + handlers (mod.rs builds Context; one file per group)
  providers/         Provider trait, Chain fallback, ProviderKind; one subdir per source
    yahoo/           chart, v7 quote (cookie/crumb session), quoteSummary (summary.rs), timeseries.rs
    msn/             autosuggest lookup -> Quotes / Equities (public web key)
  analysis/          technical.rs (indicators + signal), fundamental.rs (verdict), screen.rs (filter DSL),
                     sentiment.rs (lexicon); no network
  universe.rs        bundled data/universe.csv (IDX issuers) + ownership-db issuers; IDX_UNIVERSE override
    news/            Kontan + CNBC Indonesia RSS feeds through the Fetcher (rss crate)
  service.rs         provider call + analysis, shared by cli/ and mcp/ (fundamentals, technical, compare)
  ownership/         KSEI >1% register: discover.rs (idx.co.id page, Cloudflare strategy), ingest.rs (XLSX/PDF),
                     normalize.rs (entity keys), db.rs (SQLite schema + queries). Imported once, queried locally.
  mcp/               rmcp stdio server; one #[tool] per stocks command, same JSON envelope as -o json
  model/             provider-agnostic domain types = the JSON contract (Quote, Candle, History)
  output/            table.rs (comfy-table), json.rs (Envelope + JSON Schema), format.rs (numbers)
  http/              Fetcher: cache in front of a Backend (reqwest live / FixtureBackend tests)
  cache/             content-addressed file cache, TTL decided by the caller at read time
  config/            flags > IDX_* env > config.toml > defaults
  error.rs           IdxError with stable exit codes
tests/
  cli.rs             assert_cmd tests of the binary, no network
  mcp.rs             spawns `idx mcp`, speaks JSON-RPC over stdio, offline
  fixtures/<provider>/  recorded raw responses used by unit tests
scripts/smoke.sh     hits live providers; run manually, never in CI's test job
docs/                json-output.md, configuration.md
web/                 Cloudflare Worker for ihsg.midoriresearch.com (holding page now; D1 terminal in milestone 6)
```

Planned (later milestones): `src/analysis/screen`,

and the D1-backed terminal inside `web/`.

## Rules that keep the tool trustworthy

1. **Never invent fields.** If a source does not return a value, the field is
   `null`. No estimates, no fallbacks to another metric under the same name.
2. **No network in `cargo test`.** Record a raw response into
   `tests/fixtures/<provider>/` and parse that. Use `http::FixtureBackend` to
   test request flow (sessions, retries, fallbacks).
3. **Tables to stdout, diagnostics to stderr, non-zero exit on failure.**
   Exit codes are documented in `src/error.rs` and `--help`; do not reuse them
   for new meanings.
4. **JSON is a contract.** Payload shapes are versioned by the `schema` field
   (`idx.quotes.v1`). Adding an optional field is fine; renaming or removing
   one means a new version string. Every shape must be reachable through
   `idx schema <name>` (`output/json.rs`).
5. **Providers are interchangeable.** Nothing outside `src/providers/<name>/`
   may know about a source's URL or field names. The one exception is
   `src/ownership/`, which *is* the idx.co.id source module.
6. **Cache keys exclude secrets.** Session tokens (crumbs, cookies) never form
   part of a cache key; pass an explicit `cache_key` to `Fetcher::get`.

## Adding a provider

1. Probe the live endpoint by hand first (curl). Save representative raw
   responses, including an error and a not-found case, into
   `tests/fixtures/<name>/`.
2. Create `src/providers/<name>/mod.rs` with a struct holding
   `Arc<Fetcher>` and `Ttls`. Implement `Provider` (`providers/mod.rs`). Put pure
   parsers in `parse.rs` and test them against the fixtures.
3. Fetch through `self.fetcher.get(url, headers, ttl, cache_key)`. Map HTTP 404
   or a "not found" body to `IdxError::NotFound`; other failures to
   `IdxError::provider(NAME, ..)`. Leave the default `Unsupported` for
   capabilities the source lacks; `Chain` skips those.
4. Add a `ProviderKind` variant, wire it in `providers::build`, and decide its
   place in the `Auto` chain order.
5. Document the endpoints at the top of the module and in `README.md`'s data
   sources table. Add a line to `scripts/smoke.sh`.

## Adding a command

1. Add the variant and args to the relevant `src/cli/*.rs` enum. Give it an
   `after_help` with examples; agents read `--help`.
2. Add any new domain type to `src/model/` with `Serialize + JsonSchema`, a new
   `SCHEMA_*` constant and `schema_for` entry in `output/json.rs`, and a table
   renderer in `output/table.rs`.
3. Handlers call the provider, then branch on `ctx.json()`. Warnings go to
   stderr in table mode and into `envelope.warnings` in JSON mode.
4. Cover the parsing with fixtures and the CLI surface in `tests/cli.rs`.
5. Add a matching `#[tool]` in `src/mcp/mod.rs` with a one-paragraph
   description and a typed argument struct, returning the same envelope.
   Put shared logic in `src/service.rs`, never in the handler.

## Development loop

```sh
cargo build
cargo test                       # fixtures only
cargo clippy --all-targets -- -D warnings
cargo fmt
./scripts/smoke.sh               # live providers, needs network
./target/debug/idx -vv stocks quote BBCA   # -vv prints every request to stderr
```

Commits follow Conventional Commits (`feat:`, `fix:`, `docs:`, `test:`,
`chore:`, `ci:`). Keep each commit buildable.

## Milestones

1. ✅ Scaffold, `stocks quote`, `stocks history` via Yahoo, table + JSON, cache
2. ✅ MSN provider, `profile`, `financials`, `earnings`, `fundamental`, `technical`, `compare`
3. ✅ MCP server over stdio wrapping 1–2 (verified from Claude Code)
4. ✅ KSEI ownership ingest (XLSX + PDF text layer) into SQLite + queries
5. ✅ Screener, news, sentiment
6. Release pipeline (static binaries, install.sh, npm wrapper), web terminal
