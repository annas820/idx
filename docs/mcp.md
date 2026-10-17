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
| `idx_quote` | `symbols` ("BBCA,BBRI") | `idx.quotes.v1` |
| `idx_history` | `symbol`, `period?`, `interval?`, `limit?` (default 60 most recent candles, 0 = all) | `idx.history.v1` |
| `idx_profile` | `symbol` | `idx.profile.v1` |
| `idx_financials` | `symbol`, `period?` (`annual` \| `quarterly` \| `ttm`) | `idx.financials.v1` |
| `idx_earnings` | `symbol` | `idx.earnings.v1` |
| `idx_fundamental` | `symbol` | `idx.fundamentals.v1` (verdict included) |
| `idx_technical` | `symbol`, `period?` | `idx.technical.v1` |
| `idx_compare` | `symbols` | `idx.compare.v1` |

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
