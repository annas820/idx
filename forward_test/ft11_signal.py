#!/usr/bin/env python3
"""CLO Forward Test v1.1 (asing per saham) signal generator. Rules: forward_test/FT11_SPEC.md (DRAF until Bos freezes).

Usage: ft11_signal.py <ringkasan_csv_dir> <out_dir>
Reads <dir>/YYYYMMDD.csv (written by ringkasan_ingest.py), writes <out_dir>/v11_<D>.json. Standard library only.
"""
import csv
import glob
import json
import os
import random
import sys

UNIVERSE_N, WINDOW = 100, 5
RATIO_MIN, POS_DAYS_MIN, BREADTH_MIN = 0.10, 3, 0.40
ATR_N, MIN_SESSIONS = 14, 14
LIMIT_ATR, STOP_ATR, TGT_ATR, VALID, HOLD, COST = 0.5, 2.0, 4.0, 3, 10, 0.007
FLOAT = ["prev", "open", "high", "low", "close", "volume", "value", "freq", "foreign_sell", "foreign_buy", "listed"]


def load(csv_dir):
    days = {}
    for p in sorted(glob.glob(os.path.join(csv_dir, "*.csv"))):
        d = os.path.basename(p)[:-4]
        with open(p, encoding="utf-8") as f:
            days[d] = {r["code"]: {k: float(r[k]) for k in FLOAT} for r in csv.DictReader(f)}
    return days


def atr(series):
    """Wilder ATR over one stock's list of daily dicts; None if fewer than ATR_N traded days."""
    a, n = None, 0
    for r in series:
        if r["high"] <= 0 or r["low"] <= 0 or r["prev"] <= 0:
            continue
        tr = max(r["high"] - r["low"], abs(r["high"] - r["prev"]), abs(r["low"] - r["prev"]))
        a = tr if a is None else a + (tr - a) / ATR_N
        n += 1
    return a if n >= ATR_N else None


def build(days):
    dates = sorted(days)
    if not dates:
        return {"version": "CLO-FT-1.1-DRAFT", "status": "tanpa_data", "signals": [], "controls": []}
    D = dates[-1]
    res = {"version": "CLO-FT-1.1-DRAFT", "signal_date": D, "sessions": len(dates), "signals": [], "controls": []}
    if len(dates) < WINDOW:
        res["status"] = f"data_kurang ({len(dates)}/{WINDOW} sesi untuk jendela asing)"
        return res
    win = dates[-WINDOW:]
    total = {}
    for c, r in days[D].items():
        if r["value"] > 0 and all(c in days[d] for d in win):
            total[c] = sum(days[d][c]["value"] for d in win)
    universe = sorted(total, key=lambda c: -total[c])[:UNIVERSE_N]
    ref = dates[-WINDOW]
    base = [c for c in universe if c in days[ref] and days[ref][c]["close"] > 0]
    breadth = sum(days[D][c]["close"] > days[ref][c]["close"] for c in base) / len(base) if base else None
    today = [c for c in universe if days[D][c]["prev"] > 0]
    breadth1 = sum(days[D][c]["close"] > days[D][c]["prev"] for c in today) / len(today) if today else None
    res["breadth"] = None if breadth is None else round(breadth, 3)
    res["breadth_1d"] = None if breadth1 is None else round(breadth1, 3)
    # Both gates must pass: the 5-session one is slow, so a sharp sell-off alone must also close the gate.
    res["regime_ok"] = bool(breadth is not None and breadth1 is not None
                            and breadth >= BREADTH_MIN and breadth1 >= BREADTH_MIN)
    res["universe"] = len(universe)

    cand = []
    for c in universe:
        nets = [(days[d][c]["foreign_buy"] - days[d][c]["foreign_sell"]) * days[d][c]["close"] for d in win]
        val = sum(days[d][c]["value"] for d in win)
        ratio = sum(nets) / val if val else 0.0
        pos = sum(n > 0 for n in nets)
        cand.append((c, ratio, pos))
    flagged = [x for x in cand if x[1] >= RATIO_MIN and x[2] >= POS_DAYS_MIN]
    res["would_be_flagged"] = [{"symbol": c, "ratio_5d": round(r, 3), "positive_sessions": p}
                               for c, r, p in sorted(flagged, key=lambda x: -x[1])]

    series = {c: [days[d][c] for d in dates if c in days[d]] for c in universe}
    A = {c: atr(series[c]) for c in universe}
    if len(dates) < MIN_SESSIONS:
        res["status"] = f"data_kurang ({len(dates)}/{MIN_SESSIONS} sesi untuk ATR); tidak ada sinyal"
        return res
    if not res["regime_ok"]:
        res["status"] = "breadth di bawah 40% (5 sesi atau 1 hari): tidak ada sinyal"
    else:
        res["status"] = "ok"

    def plan(c):
        close, a = days[D][c]["close"], A[c]
        return {"symbol": c, "signal_date": D, "close": close, "atr": round(a, 2),
                "limit": round(close - LIMIT_ATR * a, 2), "valid_sessions": VALID, "stop_atr": STOP_ATR,
                "target_atr": TGT_ATR, "hold": HOLD, "cost": COST}

    ok = [c for c in universe if A[c]]
    if res["regime_ok"]:
        for c, r, p in flagged:
            if A.get(c):
                s = plan(c)
                s.update(ratio_5d=round(r, 3), positive_sessions=p)
                res["signals"].append(s)
    rng = random.Random(int(D))
    k = max(len(res["signals"]), 5)
    res["controls"] = [plan(c) for c in rng.sample(sorted(ok), min(k, len(ok)))]
    return res


def main(argv):
    if len(argv) != 3:
        print(__doc__)
        return 0
    res = build(load(argv[1]))
    os.makedirs(argv[2], exist_ok=True)
    out = os.path.join(argv[2], f"v11_{res.get('signal_date', 'none')}.json")
    with open(out, "w", encoding="utf-8") as f:
        json.dump(res, f, indent=1, ensure_ascii=False)
    print(res.get("signal_date"), "status:", res.get("status"), "breadth5:", res.get("breadth"), "breadth1:", res.get("breadth_1d"),
          "signals:", len(res["signals"]), "flagged(info):", len(res.get("would_be_flagged", [])))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
