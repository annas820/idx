#!/usr/bin/env bash
# Live smoke test against the real providers. Not run by `cargo test`.
# Usage: scripts/smoke.sh [path/to/idx]
# Needs only bash and grep; every check fails loudly.
set -euo pipefail

IDX="${1:-}"
if [ -z "$IDX" ]; then
  cargo build --quiet
  IDX="target/debug/idx"
  [ -x "$IDX" ] || IDX="target/debug/idx.exe"
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
export IDX_CACHE_DIR="$TMP/cache"
export IDX_CONFIG="$TMP/config.toml"
export NO_COLOR=1

pass() { printf '  \033[32mok\033[0m  %s\n' "$1"; }
fail() { printf '  \033[31mFAIL\033[0m %s\n' "$1"; exit 1; }
# expect <file> <fixed-string> <label>
expect() { grep -qF -- "$2" "$1" && pass "$3" || { echo "--- $1"; cat "$1"; fail "$3"; }; }

echo "yahoo"
"$IDX" stocks quote BBCA,BBRI,TLKM >"$TMP/q.txt" || fail "quote table exit $?"
expect "$TMP/q.txt" "BBCA" "quote table has BBCA"
expect "$TMP/q.txt" "Last trade" "quote table has footer"

"$IDX" -o json stocks quote BBCA >"$TMP/q.json" || fail "quote json"
expect "$TMP/q.json" '"schema": "idx.quotes.v1"' "quote json schema"
expect "$TMP/q.json" '"symbol": "BBCA"' "quote json symbol"
expect "$TMP/q.json" '"source": "yahoo:quote"' "quote json came from the full v7 endpoint"
grep -q '"price": null' "$TMP/q.json" && fail "quote json price is null" || pass "quote json has price"

"$IDX" -o json stocks history TLKM --period 1mo >"$TMP/h.json" || fail "history json"
expect "$TMP/h.json" '"schema": "idx.history.v1"' "history json schema"
expect "$TMP/h.json" '"adj_close"' "history has candles"
n="$(grep -c '"close":' "$TMP/h.json")"
[ "$n" -gt 10 ] && pass "history has $n candles" || fail "history has only $n candles"

"$IDX" stocks history BBRI --period 5d --interval 1h --limit 3 >"$TMP/i.txt" || fail "intraday history"
expect "$TMP/i.txt" "Time (WIB)" "intraday history uses time column"

set +e
"$IDX" stocks quote ZZZZ >/dev/null 2>"$TMP/e.txt"; code=$?
set -e
[ "$code" = 4 ] && pass "unknown symbol exits 4" || fail "unknown symbol exit $code"
expect "$TMP/e.txt" "not found" "unknown symbol message"

"$IDX" --offline -o json stocks quote BBCA >"$TMP/off.json" || fail "offline cached quote"
expect "$TMP/off.json" '"cached": true' "offline served from cache"

"$IDX" -o json stocks quote BBCA,ZZZZ >"$TMP/p.json" || fail "partial quote"
expect "$TMP/p.json" 'no data for ZZZZ' "partial quote lists the unknown symbol in warnings"

echo "all live checks passed"
