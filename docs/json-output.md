# JSON output

Add `-o json` (or `IDX_OUTPUT=json`, or `output = "json"` in the config file)
and every command prints one JSON document to stdout. Errors go to stderr as
JSON too. Nothing else is ever written to stdout in JSON mode, so
`idx -o json ... | jq` is safe.

Machine-readable schemas: `idx schema quotes`, `idx schema history`,
`idx schema error`, `idx schema quote`, `idx schema candle`.

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
| 6 | `config` | bad config file, env value, or unavailable provider |

## Compatibility promise

- Adding an optional field to a payload does not bump the `schema` version.
- Renaming, removing or changing the type/unit of a field does, and the old
  version is kept for at least one minor release behind a flag.
- Numbers are plain JSON numbers (no strings), timestamps are RFC 3339.
