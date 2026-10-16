#!/usr/bin/env bash
# Live smoke test against the real providers. Not run by `cargo test`.
# Usage: scripts/smoke.sh [path/to/idx]
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

PY=python3; command -v python3 >/dev/null 2>&1 || PY=python
json() { # json <expr> <file>  -- tiny jq-less extractor
  "$PY" -c 'import json,sys; d=json.load(open(sys.argv[2])); print(eval(sys.argv[1], {"d": d}))' "$1" "$2"
}

echo "yahoo"
"$IDX" stocks quote BBCA,BBRI,TLKM >"$TMP/q.txt" || fail "quote table exit $?"
grep -q "BBCA" "$TMP/q.txt" && pass "quote table has BBCA"

"$IDX" -o json stocks quote BBCA >"$TMP/q.json" || fail "quote json"
[ "$(json 'd["schema"]' "$TMP/q.json")" = "idx.quotes.v1" ] && pass "quote json schema"
[ "$(json 'd["data"][0]["symbol"]' "$TMP/q.json")" = "BBCA" ] && pass "quote json symbol"
[ "$(json 'd["data"][0]["price"] is not None' "$TMP/q.json")" = "True" ] && pass "quote json has price"

"$IDX" -o json stocks history TLKM --period 1mo >"$TMP/h.json" || fail "history json"
[ "$(json 'len(d["data"]["candles"]) > 10' "$TMP/h.json")" = "True" ] && pass "history has candles"
"$IDX" stocks history BBRI --period 5d --interval 1h --limit 3 >/dev/null && pass "intraday history"

set +e
"$IDX" stocks quote ZZZZ >/dev/null 2>&1; code=$?
set -e
[ "$code" = 4 ] && pass "unknown symbol exits 4" || fail "unknown symbol exit $code"

"$IDX" --offline -o json stocks quote BBCA >"$TMP/off.json" || fail "offline cached quote"
[ "$(json 'd["cached"]' "$TMP/off.json")" = "True" ] && pass "offline served from cache"

echo "all live checks passed"
