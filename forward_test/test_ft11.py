import unittest

from forward_test import ft11_signal as ft
from forward_test import ringkasan_ingest as ing


def row(close, prev=None, fb=0, fs=0, value=1e9, spread=10):
    prev = prev if prev is not None else close
    return {"prev": prev, "open": close, "high": close + spread, "low": close - spread, "close": close,
            "volume": 1000, "value": value, "freq": 10, "foreign_sell": fs, "foreign_buy": fb, "listed": 1e9}


def make_days(n, up=True, flow=True):
    """n sessions, 6 stocks. AAA has heavy foreign net buying; the others have none."""
    days = {}
    for i in range(n):
        d = f"2026{10 + (i // 28):02d}{(i % 28) + 1:02d}"
        px = 1000 + (i * 10 if up else -i * 10)
        day = {}
        for c in ["AAA", "BBB", "CCC", "DDD", "EEE", "FFF"]:
            day[c] = row(px, prev=px - (10 if up else -10), value=1e9)
        if flow:
            # buy 20% of value in shares at the close price: 0.2e9 / px shares
            day["AAA"]["foreign_buy"] = 0.2e9 / px
        days[d] = day
    return days


class IngestTest(unittest.TestCase):
    def test_parse_indonesian_date(self):
        self.assertEqual(ing.parse_idn_date("27 Okt 2026"), "20261027")
        self.assertEqual(ing.parse_idn_date("3 Agu 2026"), "20260803")
        self.assertIsNone(ing.parse_idn_date("27 Oct 2026"))
        self.assertIsNone(ing.parse_idn_date(None))

    def test_num_never_guesses(self):
        self.assertEqual(ing.num("0"), 0.0)
        self.assertEqual(ing.num(None), 0.0)
        self.assertEqual(ing.num("abc"), 0.0)
        self.assertEqual(ing.num(12), 12.0)


class SignalTest(unittest.TestCase):
    def test_signal_when_flow_and_breadth_ok(self):
        r = ft.build(make_days(16, up=True))
        self.assertEqual(r["status"], "ok")
        self.assertTrue(r["regime_ok"])
        self.assertEqual([s["symbol"] for s in r["signals"]], ["AAA"])
        s = r["signals"][0]
        self.assertAlmostEqual(s["ratio_5d"], 0.2, places=2)
        self.assertLess(s["limit"], s["close"])
        self.assertGreaterEqual(len(r["controls"]), 5)

    def test_low_breadth_blocks_signals(self):
        r = ft.build(make_days(16, up=False))
        self.assertFalse(r["regime_ok"])
        self.assertEqual(r["signals"], [])
        self.assertIn("breadth", r["status"])
        self.assertTrue(r["would_be_flagged"])  # reported for information only

    def test_not_enough_sessions_for_atr(self):
        r = ft.build(make_days(8, up=True))
        self.assertTrue(r["status"].startswith("data_kurang"))
        self.assertEqual(r["signals"], [])

    def test_not_enough_sessions_for_window(self):
        r = ft.build(make_days(3))
        self.assertTrue(r["status"].startswith("data_kurang"))

    def test_no_flow_no_signal(self):
        r = ft.build(make_days(16, up=True, flow=False))
        self.assertEqual(r["signals"], [])

    def test_controls_deterministic_by_date(self):
        a = ft.build(make_days(16))
        b = ft.build(make_days(16))
        self.assertEqual([c["symbol"] for c in a["controls"]], [c["symbol"] for c in b["controls"]])

    def test_empty_input(self):
        self.assertEqual(ft.build({})["status"], "tanpa_data")


if __name__ == "__main__":
    unittest.main()
