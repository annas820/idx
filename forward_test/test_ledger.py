import json
import os
import tempfile
import unittest
from datetime import datetime, timezone

from forward_test import ledger


def plan(sym, limit, atr, ratio=0.2, D="20261113"):
    return {"symbol": sym, "signal_date": D, "close": limit + 0.5 * atr, "atr": atr, "limit": limit,
            "valid_sessions": 3, "stop_atr": 2, "target_atr": 4, "hold": 10, "cost": 0.007, "ratio_5d": ratio}


def cdl(o, h, l, c):
    return {"open": o, "high": h, "low": l, "close": c}


def make_days(extra=()):
    days = {"20261113": {"AAA": cdl(1050, 1060, 1040, 1050), "BBB": cdl(5000, 5100, 4900, 5000)}}
    for d, row in extra:
        days[d] = {"AAA": cdl(*row), "BBB": cdl(5000, 5100, 4900, 5000)}
    return days


class LedgerTest(unittest.TestCase):
    def test_cheap_stock_gets_one_lot(self):
        picks, skipped, eq = ledger.choose([plan("AAA", 1000, 50)], [], make_days(), "20261113")
        self.assertEqual([p["lots"] for p in picks], [1])
        self.assertEqual(eq, 1_000_000)

    def test_expensive_lot_skipped_by_15_percent_rule(self):
        picks, skipped, _ = ledger.choose([plan("BBB", 5000, 200)], [], make_days(), "20261113")
        self.assertEqual(picks, [])
        self.assertIn("15%", skipped[0]["alasan"])

    def test_risk_cap_blocks_wide_stop(self):
        # lot 100k (ok for 15%) but stop 2*ATR=400/share -> loss 40k per lot > 2% of capital
        picks, skipped, _ = ledger.choose([plan("AAA", 1000, 200)], [], make_days(), "20261113")
        self.assertEqual(picks, [])
        self.assertIn("2%", skipped[0]["alasan"])

    def test_rank_by_ratio_and_slot_limit(self):
        sigs = [plan(f"S{i}", 500, 20, ratio=0.1 + i / 100) for i in range(6)]
        days = {"20261113": {s["symbol"]: cdl(500, 510, 490, 500) for s in sigs}}
        picks, skipped, _ = ledger.choose(sigs, [], days, "20261113")
        self.assertEqual(len(picks), ledger.MAX_POS)
        self.assertEqual(picks[0]["plan"]["symbol"], "S5")
        self.assertEqual(len(skipped), 2)

    def test_pick_file_is_never_overwritten_and_start_date_respected(self):
        with tempfile.TemporaryDirectory() as t:
            sig = os.path.join(t, "sig")
            os.makedirs(sig)
            for D in ("20261112", "20261113"):
                with open(os.path.join(sig, f"v11_{D}.json"), "w") as f:
                    json.dump({"status": "ok", "signals": [plan("AAA", 1000, 50, D=D)]}, f)
            days = make_days()
            days["20261112"] = days["20261113"]
            led = os.path.join(t, "ledger")
            now = datetime(2026, 11, 13, 10, 0, tzinfo=timezone.utc)
            self.assertEqual(ledger.make_picks(days, sig, led, now), ["20261113"])  # 12 Nov is before the start
            p = os.path.join(led, "picks", "pick_20261113.json")
            first = open(p).read()
            self.assertEqual(ledger.make_picks(days, sig, led, now), [])
            self.assertEqual(open(p).read(), first)

    def test_replay_cash_after_target(self):
        order = {"plan": plan("AAA", 1000, 50), "lots": 1}
        days = make_days([("20261116", (1010, 1020, 990, 1000)), ("20261117", (1000, 1250, 995, 1200))])
        cash, reserved, open_pos, pending = ledger.state_at([order], days, "20261117")
        expect = 1_000_000 - 100_000 * (1 + ledger.FEE_SIDE) + 120_000 * (1 - ledger.FEE_SIDE)
        self.assertAlmostEqual(cash, expect, places=2)
        self.assertEqual((reserved, open_pos, pending), (0.0, [], 0))

    def test_pending_order_reserves_cash_and_open_position_is_marked(self):
        order = {"plan": plan("AAA", 1000, 50), "lots": 1}
        cash, reserved, _, pending = ledger.state_at([order], make_days(), "20261113")
        self.assertEqual(cash, 1_000_000)
        self.assertAlmostEqual(reserved, 100_000 * (1 + ledger.FEE_SIDE), places=2)
        self.assertEqual(pending, 1)
        days = make_days([("20261116", (1010, 1020, 990, 1030))])
        cash, reserved, open_pos, _ = ledger.state_at([order], days, "20261116")
        self.assertEqual(open_pos, [("AAA", 100)])
        self.assertEqual(ledger.mark(open_pos, days, "20261116"), 103_000)

    def test_report_flags_late_pick(self):
        with tempfile.TemporaryDirectory() as t:
            led = os.path.join(t, "ledger", "picks")
            os.makedirs(led)
            order = {"plan": plan("AAA", 1000, 50), "lots": 1}
            with open(os.path.join(led, "pick_20261113.json"), "w") as f:
                json.dump({"signal_date": "20261113", "created_at": "2026-11-16T05:00:00+00:00", "note": "ok",
                           "orders": [order], "skipped": []}, f)
            days = make_days([("20261116", (1010, 1020, 990, 1030))])
            data, md = ledger.report(days, os.path.join(t, "ledger"))
            self.assertEqual(data["late_picks"], ["AAA@20261113"])
            self.assertIn("PERHATIAN", md)


if __name__ == "__main__":
    unittest.main()
