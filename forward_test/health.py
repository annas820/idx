#!/usr/bin/env python3
"""CLO data-health gate: a mechanical read of one daily snapshot.

It answers three questions before anyone reads a candidate list:
  1. Which data pillars worked (prices, calendar, news, market foreign flow, per-stock broker data)?
  2. How old is the newest usable broker data?
  3. Are new entries allowed under the hard rules (broker data present, breadth >= 40%)?

Reporting only. It never touches the frozen Forward Test v1.0 rules in ft_signal.py. The breadth here is a
price-breadth PROXY (share of quoted stocks that advanced today), not the "above MA50" breadth that the
Forward Test uses; the report labels it as such.

Usage: health.py <snapshot_dir> <snapshots_root> <out_dir>   -> writes health.json and health.md
Standard library only. Always exits 0 so a bad snapshot never blocks the commit step.
"""
import json
import os
import re
import sys

BREADTH_MIN = 0.40

PILLARS = {
    "harga": ["overview", "quotes"],
    "kalender": ["holidays"],
    "berita": ["news"],
    "arus_asing_pasar": ["foreign_mkt"],
    "broker": ["fnet_5d", "fnet_1d", "bandar"],
}
SWALLOWED_502 = "no session with trades"


def read_json(path):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def read_text(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return f.read()
    except OSError:
        return ""


def parse_status(text):
    """status.txt lines: 'OK   name' or 'FAIL name (exit N)'. Returns {name: bool}."""
    out = {}
    for line in text.splitlines():
        m = re.match(r"^(OK|FAIL)\s+(\S+)", line)
        if m:
            out[m.group(2)] = m.group(1) == "OK"
    return out


def last_good_broker(root):
    """Newest snapshot whose fnet_5d.json has rows: (folder date, data end date) or (None, None)."""
    try:
        names = sorted((n for n in os.listdir(root) if re.match(r"^\d{4}-\d{2}-\d{2}$", n)), reverse=True)
    except OSError:
        return None, None
    for n in names:
        d = read_json(os.path.join(root, n, "fnet_5d.json"))
        data = (d or {}).get("data") or {}
        if data.get("rows"):
            return n, data.get("to")
    return None, None


def build(snap_dir, root):
    day = os.path.basename(os.path.normpath(snap_dir))
    status = parse_status(read_text(os.path.join(snap_dir, "status.txt")))
    pillars, failed = {}, []
    for name, steps in PILLARS.items():
        bad = [s for s in steps if not status.get(s, False)]
        pillars[name] = "ok" if not bad else ("gagal" if len(bad) == len(steps) else "sebagian")
        failed += bad

    reasons = []
    ov = (read_json(os.path.join(snap_dir, "overview.json")) or {}).get("data") or {}
    quoted, adv = ov.get("quoted") or 0, ov.get("advancing") or 0
    breadth = round(adv / quoted, 3) if quoted else None
    as_of = (ov.get("as_of") or "")[:10]
    stale = bool(as_of) and as_of != day
    idx = ov.get("index") or {}

    note = None
    if pillars["broker"] != "ok":
        errs = " ".join(read_text(os.path.join(snap_dir, f"{s}.err")) for s in PILLARS["broker"])
        if SWALLOWED_502 in errs:
            note = ("Pesan 'no session with trades' menyesatkan: dugaan error IPOT (HTTP 502) yang tertelan. "
                    "Lihat diag/ dan STATUS.md.")
        reasons.append("data broker tidak tersedia: tidak ada sinyal yang bisa dihitung")
    if pillars["harga"] != "ok":
        reasons.append("data harga tidak lengkap")
    if stale:
        reasons.append(f"data harga basi (as_of {as_of} != {day})")
    if breadth is None:
        reasons.append("breadth tidak dapat dihitung")
    elif breadth < BREADTH_MIN:
        reasons.append(f"breadth harga {breadth:.0%} < 40% (proxy)")

    good_dir, good_to = last_good_broker(root)
    return {
        "schema": "clo.health.v1",
        "snapshot_date": day,
        "run_at": next((l for l in read_text(os.path.join(snap_dir, "status.txt")).splitlines()
                        if l.startswith("run at")), None),
        "pillars": pillars,
        "failed_steps": failed,
        "price_breadth_proxy": breadth,
        "advancing": adv,
        "quoted": quoted,
        "index_change_pct": idx.get("change_pct"),
        "last_good_broker_snapshot": good_dir,
        "last_good_broker_data_to": good_to,
        "broker_note": note,
        "entries_allowed": not reasons,
        "reasons": reasons,
    }


def render_md(h):
    gate = "BOLEH (semua syarat terpenuhi)" if h["entries_allowed"] else "TIDAK: " + "; ".join(h["reasons"])
    b = h["price_breadth_proxy"]
    lines = [
        f"# Kesehatan data {h['snapshot_date']}",
        f"- Entri baru: **{gate}**",
        "- Pilar: " + ", ".join(f"{k}={v}" for k, v in h["pillars"].items()),
        f"- Breadth harga (proxy, bukan MA50): {'n/a' if b is None else f'{b:.1%}'} "
        f"({h['advancing']} naik dari {h['quoted']})",
        f"- Data broker terbaru: {h['last_good_broker_snapshot'] or 'tidak ada'}"
        f" (sampai {h['last_good_broker_data_to'] or 'n/a'})",
    ]
    if h["broker_note"]:
        lines.append(f"- Catatan: {h['broker_note']}")
    if h["failed_steps"]:
        lines.append("- Langkah gagal: " + ", ".join(h["failed_steps"]))
    return "\n".join(lines) + "\n"


def main(argv):
    if len(argv) != 4:
        print(__doc__)
        return 0
    snap_dir, root, out = argv[1:]
    h = build(snap_dir, root)
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "health.json"), "w", encoding="utf-8") as f:
        json.dump(h, f, indent=1, ensure_ascii=False)
    with open(os.path.join(out, "health.md"), "w", encoding="utf-8") as f:
        f.write(render_md(h))
    print(render_md(h))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
