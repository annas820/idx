# Configuration

Precedence, highest first: **command-line flags → `IDX_*` environment
variables → config file → built-in defaults**.

## Config file

| platform | path |
| --- | --- |
| Linux, macOS | `$XDG_CONFIG_HOME/idx/config.toml`, else `~/.config/idx/config.toml` |
| Windows | `%APPDATA%\idx\config.toml` |

Override the location with `--config PATH` or `IDX_CONFIG=PATH`.

```sh
idx config init                       # write a commented template
idx config set provider yahoo
idx config set cache.quote_ttl_secs 120
idx config get                        # effective settings after all layers
idx config get provider               # one key, as stored in the file
idx config path
```

```toml
provider = "auto"      # auto | yahoo | msn
output = "table"       # table | json | csv
color = "auto"         # auto | always | never
timeout_secs = 15

[cache]
dir = "/custom/cache/dir"
quote_ttl_secs = 60
history_ttl_secs = 3600
```

## Environment variables

| variable | flag | meaning |
| --- | --- | --- |
| `IDX_PROVIDER` | `--provider` | `auto`, `yahoo`, `msn` |
| `IDX_OUTPUT` | `-o, --output` | `table`, `json` |
| `IDX_COLOR` | | `auto`, `always`, `never` |
| `NO_COLOR` | `--no-color` | any non-empty value disables colour |
| `IDX_TIMEOUT_SECS` | `--timeout` | per-request timeout |
| `IDX_CACHE_DIR` | `--cache-dir` | cache location |
| `IDX_OFFLINE` | `--offline` | `1`/`true`: never touch the network |
| `IDX_NO_CACHE` | `--no-cache` | `1`/`true`: ignore cached responses |
| `IDX_QUOTE_TTL_SECS` | | cache lifetime for quotes |
| `IDX_HISTORY_TTL_SECS` | | cache lifetime for history (intraday bars are capped at 120 s) |
| `IDX_CONFIG` | `--config` | config file path |
| `IDX_LOG` | `-v` | tracing filter, e.g. `idx=debug` |

## Cache

| platform | path |
| --- | --- |
| Linux, macOS | `$XDG_CACHE_HOME/idx`, else `~/.cache/idx` |
| Windows | `%LOCALAPPDATA%\idx\cache` |

Each response is stored as one JSON file named by the SHA-256 of its cache
key. Freshness is judged against the TTL in effect when you *read*, so changing
a TTL applies immediately.

- `--offline`: serve whatever is cached, fresh or stale; exit 5 on a miss.
- `--no-cache`: always fetch; the result is still written for later.
- Network failure with a stale entry present: the stale entry is served and
  `stale: true` is set (table mode prints a warning to stderr).

```sh
idx cache stats
idx cache clear
idx cache path
```

The Yahoo session (anonymous cookie + crumb) is cached for six hours under the
same directory. `idx cache clear` removes it too.
