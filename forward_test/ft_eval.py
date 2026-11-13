#!/usr/bin/env python3
"""CLO outcome evaluator for the v1.1 DRAFT plans (signals_v11/v11_*.json).

Reads the day files in data/ringkasan/YYYYMMDD.csv (code, open, high, low, close) and replays every plan and its
random controls with the execution rules of EVAL_SPEC.md / FT11_SPEC.md:
  open <= limit fills at open; low <= limit fills at limit; valid for `valid_sessions` sessions.
  Stop and target are measured from the FILL price (FT11_SPEC, "Penilaian"). Stop and target in the same candle:
  stop first. A gap through the stop exits at the open. Hold at most `hold` sessions, then the close of the last one.
  A day with open == 0 or low == 0 (IDX rows without an opening price) never counts as a fill.
Reporting only; standard library only. Usage: ft_eval.py <ringkasan_dir> <signals_dir> <out_dir>  -> eval_v11.md
"""
import csv
import glob
import json
import os
import statistics as st
import sys

FIELDS = ["open", "high", "low", "close"]


def load_days(csv_dir):
    days = {}
    for p in sorted(glob.glob(os.path.join(csv_dir, "*.csv"))):
        d = os.path.basename(p)[:-4]
        with open(p, encoding="utf-8") as f:
            days[d] = {r["code"]: {k: float(r[k]) for k in FIELDS} for r in csv.DictReader(f)}
    return days


def evaluate(plan, days):
    order = sorted(days)
    sym, sig = plan["symbol"], plan["signal_date"]
    later = [d for d in order if d > sig and sym in days[d] and days[d][sym]["high"] > 0]
    if not later:
        return {"status": "belum_ada_data"}
    atr, lim = plan["atr"], plan["limit"]
    entry = fill_i = None
    for i, d in enumerate(later[: plan["valid_sessions"]]):
        c = days[d][sym]
        if 0 < c["open"] <= lim:
            entry, fill_i = c["open"], i
            break
        if 0 < c["low"] <= lim:
            entry, fill_i = lim, i
            break
    if entry is None:
        return {"status": "tidak_terisi" if len(later) >= plan["valid_sessions"] else "menunggu_isi"}
    risk = plan["stop_atr"] * atr
    stop, tgt = entry - risk, entry + plan["target_atr"] * atr
    window = later[fill_i: fill_i + plan["hold"]]
    exit_p = why = None
    for j, d in enumerate(window):
        c = days[d][sym]
        if c["low"] > 0 and c["low"] <= stop:
            exit_p, why = (min(c["open"], stop) if j > 0 and c["open"] > 0 else stop), "kena_stop"
            break
        if c["high"] >= tgt:
            exit_p, why = tgt, "kena_target"
            break
    if exit_p is None:
        exit_p = days[window[-1]][sym]["close"]
        why = "tahan_habis" if len(window) >= plan["hold"] else "terbuka"
    cost = plan["cost"]
    return {"status": why, "entry": round(entry, 2), "exit": round(exit_p, 2),
            "net": round(exit_p / entry - 1 - cost, 4),
            "r": round((exit_p - entry - cost * entry) / risk, 2)}


def summarize(plans, days):
    res = [(p["symbol"], evaluate(p, days)) for p in plans]
    filled = [r for _, r in res if "net" in r]
    counts = {}
    for _, r in res:
        counts[r["status"]] = counts.get(r["status"], 0) + 1
    return {"counts": counts, "n_filled": len(filled),
            "mean_net": st.mean(r["net"] for r in filled) if filled else None,
            "mean_r": st.mean(r["r"] for r in filled) if filled else None,
            "rows": res}


def build(days, sig_dir):
    out = []
    for p in sorted(glob.glob(os.path.join(sig_dir, "v11_*.json"))):
        try:
            with open(p, encoding="utf-8") as f:
                s = json.load(f)
        except (OSError, ValueError):
            continue
        if not s.get("signals"):
            continue
        D = s["signal_date"]
        out.append({"date": D, "sessions_after": len([d for d in days if d > D]),
                    "signals": summarize(s["signals"], days), "controls": summarize(s.get("controls") or [], days)})
    return out


def pct(x):
    return "n/a" if x is None else f"{x * 100:+.2f}%"


def render_md(report):
    lines = ["# Penilaian hasil v1.1 (LATIHAN: spec masih DRAF, bukan uji maju, jangan disebut sinyal)",
             f"- Hari sinyal yang dinilai: {len(report)}. Di bawah 20 hari sinyal tidak boleh ada klaim statistik (EVAL_SPEC).",
             "- Stop/target dihitung dari harga isi. Harga buka 0 tidak dihitung sebagai isi. Posisi terbuka dinilai pada penutupan terakhir.",
             "- Biaya 0,7% sudah dikurangkan. Selisih = sinyal minus kontrol per hari sinyal.", ""]
    for r in report:
        s, c = r["signals"], r["controls"]
        diff = None if s["mean_net"] is None or c["mean_net"] is None else s["mean_net"] - c["mean_net"]
        lines += [f"## Hari sinyal {r['date']} ({r['sessions_after']} sesi sesudahnya)",
                  f"- Sinyal: terisi {s['n_filled']}, status {s['counts']}, rata-rata bersih {pct(s['mean_net'])}, "
                  f"rata-rata R {'n/a' if s['mean_r'] is None else f'{s['mean_r']:+.2f}'}",
                  f"- Kontrol: terisi {c['n_filled']}, status {c['counts']}, rata-rata bersih {pct(c['mean_net'])}",
                  f"- Selisih: {pct(diff)}", ""]
    return "\n".join(lines)


def main(argv):
    if len(argv) != 4:
        print(__doc__)
        return 0
    days = load_days(argv[1])
    rep = build(days, argv[2])
    os.makedirs(argv[3], exist_ok=True)
    md = render_md(rep)
    with open(os.path.join(argv[3], "eval_v11.md"), "w", encoding="utf-8") as f:
        f.write(md)
    print(md)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
