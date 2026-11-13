#!/usr/bin/env python3
"""CLO paper-trading ledger, Rp1,000,000 start (rules: ledger/RULES.md, written BEFORE the first pick).

Two things, both deterministic:
  pick   For every v1.1 signal day D >= START_SIGNAL_DATE that has no ledger/picks/pick_D.json yet, choose orders with the
         fixed rules and write the file. A pick file is never overwritten, so a rule or a choice cannot be changed after
         the outcome is known. The file records created_at (UTC); a pick created after the first fill session opened is
         flagged "terlambat" in the report.
  report Replays all pick files against data/ringkasan/*.csv (fills and exits via ft_eval.evaluate) and writes
         latest/ledger_1jt.md and latest/ledger_1jt.json.
Reporting/simulation only. Prices ignore the IDX tick size and partial fills; fees are the 0.7% round trip of the spec split in
two (the real IPOT fee is not verified). Usage: ledger.py <ringkasan_dir> <signals_dir> <ledger_dir> <out_dir>
"""
import glob
import json
import math
import os
import sys
from datetime import datetime, timedelta, timezone

try:
    from forward_test import ft_eval
except ImportError:  # run as a script
    import ft_eval

CAPITAL = 1_000_000
START_SIGNAL_DATE = "20261113"
POS_CAP = 0.15          # max share of equity in one stock (START.md: 10-15%)
RISK_CAP = 0.02         # max loss at the stop, as share of equity, for the whole position
MAX_POS = 4             # open positions + pending orders
FEE_SIDE = 0.0035       # half of the 0.7% round-trip cost per side
LOT = 100
WIB = timezone(timedelta(hours=7))


def next_day_open(date, order):
    """Opening time (WIB) of the first session after signal day `date`, from the days that exist or the next weekday."""
    later = [d for d in order if d > date]
    if later:
        d = later[0]
    else:
        t = datetime.strptime(date, "%Y%m%d")
        t += timedelta(days=3 if t.weekday() == 4 else 1)
        d = t.strftime("%Y%m%d")
    return datetime.strptime(d, "%Y%m%d").replace(hour=9, tzinfo=WIB)


def lifecycle(order, days):
    r = ft_eval.evaluate(order["plan"], days)
    r["lots"] = order["lots"]
    return r


def state_at(orders, days, upto):
    """Cash, reserved cash and open positions at the close of `upto` (YYYYMMDD) from the orders made on or before it."""
    cash, reserved, open_pos, pending = CAPITAL, 0.0, [], 0
    for o in orders:
        D = o["plan"]["signal_date"]
        if D > upto:
            continue
        r = lifecycle(o, days)
        shares = o["lots"] * LOT
        filled_by = r.get("fill_date") and r["fill_date"] <= upto
        if filled_by:
            cash -= shares * r["entry"] * (1 + FEE_SIDE)
            if r["status"] in ("kena_stop", "kena_target", "tahan_habis") and r["exit_date"] <= upto:
                cash += shares * r["exit"] * (1 - FEE_SIDE)
            else:
                open_pos.append((o["plan"]["symbol"], shares))
        else:
            expired = r.get("expiry_date") and r["expiry_date"] <= upto
            if not expired:
                reserved += shares * o["plan"]["limit"] * (1 + FEE_SIDE)
                pending += 1
    return cash, reserved, open_pos, pending


def mark(open_pos, days, upto):
    total = 0.0
    for sym, shares in open_pos:
        px = None
        for d in sorted(days, reverse=True):
            if d <= upto and sym in days[d] and days[d][sym]["close"] > 0:
                px = days[d][sym]["close"]
                break
        total += shares * (px or 0)
    return total


def choose(signals, orders, days, D):
    cash, reserved, open_pos, pending = state_at(orders, days, D)
    equity = cash + mark(open_pos, days, D)
    free = cash - reserved
    slots = MAX_POS - len(open_pos) - pending
    picks, skipped = [], []
    for s in sorted(signals, key=lambda x: (-x["ratio_5d"], x["symbol"])):
        px, risk = s["limit"], s["stop_atr"] * s["atr"]
        if slots <= 0:
            skipped.append({"symbol": s["symbol"], "alasan": "slot penuh"})
            continue
        by_pos = math.floor(POS_CAP * equity / (LOT * px))
        by_risk = math.floor(RISK_CAP * equity / (LOT * (risk + 2 * FEE_SIDE * px)))
        by_cash = math.floor(free / (LOT * px * (1 + FEE_SIDE)))
        lots = min(by_pos, by_risk, by_cash)
        if lots < 1:
            why = "1 lot melebihi batas 15% modal" if by_pos < 1 else (
                "rugi di stop 1 lot melebihi 2% modal" if by_risk < 1 else "kas tidak cukup")
            skipped.append({"symbol": s["symbol"], "alasan": why})
            continue
        free -= lots * LOT * px * (1 + FEE_SIDE)
        slots -= 1
        picks.append({"plan": s, "lots": lots})
    return picks, skipped, round(equity)


def load_orders(ledger_dir):
    orders = []
    for p in sorted(glob.glob(os.path.join(ledger_dir, "picks", "pick_*.json"))):
        with open(p, encoding="utf-8") as f:
            pk = json.load(f)
        orders += [dict(o, created_at=pk.get("created_at"), signal_date=pk["signal_date"]) for o in pk["orders"]]
    return orders


def make_picks(days, sig_dir, ledger_dir, now=None):
    os.makedirs(os.path.join(ledger_dir, "picks"), exist_ok=True)
    now = now or datetime.now(timezone.utc)
    made = []
    for p in sorted(glob.glob(os.path.join(sig_dir, "v11_*.json"))):
        D = os.path.basename(p)[4:12]
        out = os.path.join(ledger_dir, "picks", f"pick_{D}.json")
        if D < START_SIGNAL_DATE or os.path.exists(out) or D not in days:
            continue
        with open(p, encoding="utf-8") as f:
            sig = json.load(f)
        if sig.get("status") != "ok" or not sig.get("signals"):
            picks, skipped, equity = [], [], None
            note = sig.get("status") or "tidak ada sinyal"
        else:
            picks, skipped, equity = choose(sig["signals"], load_orders(ledger_dir), days, D)
            note = "ok"
        with open(out, "w", encoding="utf-8") as f:
            json.dump({"schema": "clo.ledger.pick.v1", "signal_date": D, "created_at": now.isoformat(timespec="seconds"),
                       "equity_at_close": equity, "note": note, "orders": picks, "skipped": skipped}, f,
                      indent=1, ensure_ascii=False)
        made.append(D)
    return made


def rp(x):
    return f"Rp{x:,.0f}".replace(",", ".")


def report(days, ledger_dir):
    orders = load_orders(ledger_dir)
    order_days = sorted(days)
    last = order_days[-1]
    cash, reserved, open_pos, pending = state_at(orders, days, last)
    equity = cash + mark(open_pos, days, last)
    rows, late = [], []
    peak, mdd = CAPITAL, 0.0
    for d in order_days:
        if d < START_SIGNAL_DATE:
            continue
        c, _, op, _ = state_at(orders, days, d)
        e = c + mark(op, days, d)
        peak = max(peak, e)
        mdd = max(mdd, (peak - e) / peak)
    for o in orders:
        r = lifecycle(o, days)
        plan = o["plan"]
        if o.get("created_at"):
            created = datetime.fromisoformat(o["created_at"])
            if created > next_day_open(plan["signal_date"], order_days):
                late.append(plan["symbol"] + "@" + plan["signal_date"])
        rows.append((plan["signal_date"], plan["symbol"], o["lots"], plan["limit"], r))
    closed = [r for *_, r in rows if r["status"] in ("kena_stop", "kena_target", "tahan_habis")]
    wins = [r for r in closed if r["net"] > 0]
    data = {"schema": "clo.ledger.v1", "as_of": last, "capital_start": CAPITAL, "cash": round(cash),
            "equity": round(equity), "return_pct": round((equity / CAPITAL - 1) * 100, 2),
            "open_positions": len(open_pos), "pending_orders": pending, "closed_trades": len(closed),
            "wins": len(wins), "max_drawdown_pct": round(mdd * 100, 2), "late_picks": late}
    L = [f"# Buku simulasi Rp1 juta (LATIHAN, uang bayangan) — data sampai {last}",
         f"- Modal awal {rp(CAPITAL)}; nilai sekarang **{rp(equity)}** ({data['return_pct']:+.2f}%); kas {rp(cash)}",
         f"- Posisi terbuka {len(open_pos)}, order menunggu {pending}, transaksi selesai {len(closed)} (menang {len(wins)})",
         f"- Penurunan terdalam sejak mulai: {mdd * 100:.2f}%",
         "- Aturan: `ledger/RULES.md` (ditulis sebelum pick pertama). Di bawah 20 hari sinyal tanpa klaim apa pun; ini bukan bukti strategi.",
         "- Biaya: 0,7% pulang-pergi (asumsi spec, biaya IPOT sebenarnya belum diverifikasi). Tick harga dan pengisian sebagian diabaikan."]
    if late:
        L.append("- PERHATIAN: pick dibuat setelah sesi pertama dibuka (nilai dengan catatan): " + ", ".join(late))
    L += ["", "## Order dan transaksi", "| Hari sinyal | Saham | Lot | Limit | Status | Masuk | Keluar | Bersih |", "|---|---|---|---|---|---|---|---|"]
    for D, sym, lots, lim, r in rows:
        L.append(f"| {D} | {sym} | {lots} | {lim} | {r['status']} | {r.get('entry', '')} | {r.get('exit', '')} | "
                 f"{'' if 'net' not in r else f'{r['net'] * 100:+.2f}%'} |")
    if not rows:
        L.append("| (belum ada order) | | | | | | | |")
    L += ["", "## Pick yang dilewati"]
    for p in sorted(glob.glob(os.path.join(ledger_dir, "picks", "pick_*.json"))):
        with open(p, encoding="utf-8") as f:
            pk = json.load(f)
        if pk.get("skipped") or not pk["orders"]:
            L.append(f"- {pk['signal_date']}: {pk['note']}" + "; " + ", ".join(
                f"{s['symbol']} ({s['alasan']})" for s in pk.get("skipped", [])))
    return data, "\n".join(L) + "\n"


def main(argv):
    if len(argv) != 5:
        print(__doc__)
        return 0
    days = ft_eval.load_days(argv[1])
    made = make_picks(days, argv[2], argv[3])
    data, md = report(days, argv[3])
    os.makedirs(argv[4], exist_ok=True)
    with open(os.path.join(argv[4], "ledger_1jt.md"), "w", encoding="utf-8") as f:
        f.write(md)
    with open(os.path.join(argv[4], "ledger_1jt.json"), "w", encoding="utf-8") as f:
        json.dump(data, f, indent=1, ensure_ascii=False)
    print("pick baru:", made or "-")
    print(md)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
