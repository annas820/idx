#!/usr/bin/env python3
"""Ingest hand-downloaded IDX 'Ringkasan Saham' xlsx files into small normalized CSVs.

Usage: ringkasan_ingest.py <manual_dir> <out_dir>
Reads  <manual_dir>/Ringkasan Saham-YYYYMMDD.xlsx   (needs openpyxl)
Writes <out_dir>/YYYYMMDD.csv  with columns in COLS. A file is rejected (reported, not written) if its header lacks a
required column or if the trading date inside the file differs from the date in its name. Never guesses values.
"""
import csv
import glob
import os
import re
import sys
from collections import Counter

COLS = ["date", "code", "prev", "open", "high", "low", "close", "volume", "value", "freq",
        "foreign_sell", "foreign_buy", "listed"]
SRC = {"code": "Kode Saham", "prev": "Sebelumnya", "open": "Open Price", "high": "Tertinggi", "low": "Terendah",
       "close": "Penutupan", "volume": "Volume", "value": "Nilai", "freq": "Frekuensi",
       "foreign_sell": "Foreign Sell", "foreign_buy": "Foreign Buy", "listed": "Listed Shares"}
MONTHS = {"Jan": 1, "Feb": 2, "Mar": 3, "Apr": 4, "Mei": 5, "Jun": 6, "Jul": 7, "Agu": 8, "Sep": 9, "Okt": 10,
          "Nov": 11, "Des": 12}


def num(v):
    if v is None or v == "":
        return 0.0
    try:
        return float(v)
    except (TypeError, ValueError):
        return 0.0


def parse_idn_date(s):
    m = re.match(r"^\s*(\d{1,2})\s+([A-Za-z]{3})\s+(\d{4})\s*$", str(s or ""))
    if not m or m.group(2).title() not in MONTHS:
        return None
    return f"{int(m.group(3)):04d}{MONTHS[m.group(2).title()]:02d}{int(m.group(1)):02d}"


def date_error(seen, day, total):
    """None if at least 90% of rows carry the trade date `day`; otherwise a message. `seen` counts parsed dates."""
    if not seen:
        return "tidak ada baris bertanggal"
    top, count = seen.most_common(1)[0]
    if top != day:
        return f"tanggal di dalam file ({top}) berbeda dari nama file ({day})"
    if count < 0.9 * total:
        return f"hanya {count} dari {total} baris bertanggal {day}"
    return None


def read_rows(path):
    import openpyxl
    ws = openpyxl.load_workbook(path, read_only=True, data_only=True).worksheets[0]
    rows = list(ws.iter_rows(values_only=True))
    return rows[0], rows[1:]


def convert(path, day):
    """Return (list of dict rows, error string or None)."""
    header, body = read_rows(path)
    ix = {h: i for i, h in enumerate(header) if h}
    need = list(SRC.values()) + ["Tanggal Perdagangan Terakhir"]
    missing = [c for c in need if c not in ix]
    if missing:
        return [], f"kolom hilang: {missing}"
    body = [r for r in body if r[ix["Kode Saham"]]]
    seen = Counter(parse_idn_date(r[ix["Tanggal Perdagangan Terakhir"]]) for r in body)
    err = date_error(seen, day, len(body))
    if err:
        return [], err
    out = []
    for r in body:
        d = {"date": day, "code": str(r[ix["Kode Saham"]]).strip()}
        for k, src in SRC.items():
            if k != "code":
                d[k] = num(r[ix[src]])
        out.append(d)
    return out, None


def main(argv):
    if len(argv) != 3:
        print(__doc__)
        return 0
    manual, out = argv[1:]
    os.makedirs(out, exist_ok=True)
    for p in sorted(glob.glob(os.path.join(manual, "Ringkasan Saham-*.xlsx"))):
        m = re.search(r"Ringkasan Saham-(\d{8})\.xlsx$", p)
        if not m:
            print("LEWATI (nama tidak sesuai):", os.path.basename(p))
            continue
        day, dest = m.group(1), os.path.join(out, m.group(1) + ".csv")
        if os.path.exists(dest) and os.path.getmtime(dest) >= os.path.getmtime(p):
            continue
        try:
            rows, err = convert(p, day)
        except Exception as e:  # unreadable file must never stop the daily run
            rows, err = [], f"tidak bisa dibaca: {e}"
        if err:
            print("DITOLAK", os.path.basename(p), "-", err)
            continue
        with open(dest, "w", newline="", encoding="utf-8") as f:
            w = csv.DictWriter(f, fieldnames=COLS)
            w.writeheader()
            w.writerows(rows)
        print("OK", day, len(rows), "saham")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
