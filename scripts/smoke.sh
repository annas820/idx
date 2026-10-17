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

echo "fundamentals"
"$IDX" stocks fundamental BBCA >"$TMP/f.txt" || fail "fundamental table"
expect "$TMP/f.txt" "VALUATION" "fundamental table has sections"
expect "$TMP/f.txt" "Heuristic" "fundamental table has the verdict line"
"$IDX" -o json stocks fundamental TLKM >"$TMP/f.json" || fail "fundamental json"
expect "$TMP/f.json" '"schema": "idx.fundamentals.v1"' "fundamental json schema"
expect "$TMP/f.json" '"yahoo:quoteSummary"' "fundamental json came from Yahoo"
grep -q '"pe_trailing": null' "$TMP/f.json" && fail "TLKM P/E is null" || pass "fundamental json has P/E"

"$IDX" -o json stocks profile BBCA >"$TMP/pr.json" || fail "profile json"
expect "$TMP/pr.json" '"sector": "Financial Services"' "profile sector"

"$IDX" -o json stocks financials BBCA --period quarterly >"$TMP/fin.json" || fail "financials json"
expect "$TMP/fin.json" '"period": "quarterly"' "financials quarterly"
expect "$TMP/fin.json" '"net_income":' "financials have net income"

"$IDX" -o json stocks earnings BBCA >"$TMP/e.json" || fail "earnings json"
expect "$TMP/e.json" '"eps_actual":' "earnings have reported EPS"

"$IDX" stocks technical BBRI >"$TMP/t.txt" || fail "technical table"
expect "$TMP/t.txt" "RSI (14)" "technical table has RSI"
expect "$TMP/t.txt" "Why:" "technical table explains the signal"

"$IDX" stocks compare BBCA,BBRI,TLKM >"$TMP/c.txt" || fail "compare table"
expect "$TMP/c.txt" "Verdict" "compare table has verdict row"

echo "msn"
"$IDX" --provider msn -o json stocks quote BBCA,TLKM >"$TMP/mq.json" || fail "msn quote"
expect "$TMP/mq.json" '"provider": "msn"' "msn quote provider"
expect "$TMP/mq.json" '"source": "msn:quotes"' "msn quote source"
"$IDX" --provider msn -o json stocks fundamental BBCA >"$TMP/mf.json" || fail "msn fundamental"
expect "$TMP/mf.json" '"msn:equities"' "msn fundamental source"
expect "$TMP/mf.json" '"recommendation":' "msn fundamental has analyst block"

echo "screen / news / sentiment"
"$IDX" stocks screen -f "mcap>100T,pe<20" --limit 10 >"$TMP/sc.txt" || fail "screen"
expect "$TMP/sc.txt" "Mkt Cap" "screen table renders"
expect "$TMP/sc.txt" "tickers quoted" "screen reports the funnel"
"$IDX" -o json stocks screen -f "roe>15,mcap>50T" --max-fundamentals 20 --limit 5 >"$TMP/sc.json" || fail "screen with fundamentals"
expect "$TMP/sc.json" '"fundamentals_checked":' "screen json has fundamentals count"
"$IDX" stocks news --limit 5 >"$TMP/nw.txt" || fail "news"
expect "$TMP/nw.txt" "Headline" "news table renders"
"$IDX" -o json stocks sentiment BBCA >"$TMP/se.json" || fail "sentiment"
expect "$TMP/se.json" '"schema": "idx.sentiment.v1"' "sentiment json schema"
expect "$TMP/se.json" '"label":' "sentiment json has a label"
"$IDX" -o json stocks news BBCA --limit 5 >"$TMP/nwc.json" || fail "news with symbol"
expect "$TMP/nwc.json" '"company_matches":' "news json reports company matches"

echo "dividends, returns, market, brief (0.2.0)"
"$IDX" -o json stocks dividends BBCA >"$TMP/dv.json" || fail "dividends"
expect "$TMP/dv.json" '"schema": "idx.dividends.v1"' "dividends json schema"
expect "$TMP/dv.json" '"ttm_yield_pct":' "dividends json has a TTM yield"
"$IDX" -o json stocks returns BBCA BBRI >"$TMP/rt.json" || fail "returns"
expect "$TMP/rt.json" '"excess":' "returns json has benchmark excess"
"$IDX" -o csv stocks returns BBCA --no-benchmark >"$TMP/rt.csv" || fail "returns csv"
expect "$TMP/rt.csv" "returns.1y" "csv flattens nested columns"
"$IDX" -o json market overview --top 3 >"$TMP/mk.json" || fail "market overview"
expect "$TMP/mk.json" '"advancing":' "market json has breadth"
"$IDX" -o json stocks brief BBCA >"$TMP/br.json" || fail "brief"
expect "$TMP/br.json" '"key_points":' "brief json has key points"

echo "ownership (local fixture)"
export IDX_OWNERSHIP_DB="$TMP/own.sqlite"
"$IDX" ownership import --file tests/fixtures/ownership/above1_20260831_excerpt.xlsx >"$TMP/oi.txt" || fail "ownership import"
expect "$TMP/oi.txt" "Imported 2026-08-31" "ownership import summary"
"$IDX" ownership ticker AADI >"$TMP/ot.txt" || fail "ownership ticker"
expect "$TMP/ot.txt" "ADARO STRATEGIC INVESTMENTS" "ownership ticker lists the top holder"
set +e
"$IDX" ownership discover >"$TMP/od.txt" 2>"$TMP/od.err"; code=$?
set -e
if [ "$code" = 0 ]; then pass "ownership discover reached idx.co.id"; else echo "  skip ownership discover (exit $code): $(head -c 160 "$TMP/od.err")"; fi

echo "all live checks passed"
