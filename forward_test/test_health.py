import json
import os
import tempfile
import unittest

from forward_test import health


def make_snapshot(root, day, status, advancing=300, quoted=600, as_of=None, fnet_rows=0, errs=None):
    d = os.path.join(root, day)
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, "status.txt"), "w") as f:
        f.write(status + "\nrun at 2026-10-27 11:01:00 UTC\n")
    ov = {"data": {"as_of": (as_of or day) + "T16:00:00+07:00", "advancing": advancing, "quoted": quoted,
                   "index": {"change_pct": -1.2}}}
    with open(os.path.join(d, "overview.json"), "w") as f:
        json.dump(ov, f)
    if fnet_rows:
        with open(os.path.join(d, "fnet_5d.json"), "w") as f:
            json.dump({"data": {"to": day, "rows": [{"symbol": "AAAA"}] * fnet_rows}}, f)
    for name, text in (errs or {}).items():
        with open(os.path.join(d, name + ".err"), "w") as f:
            f.write(text)
    return d


ALL_OK = "\n".join(f"OK   {s}" for s in
                   ["holidays", "overview", "quotes", "news", "foreign_mkt", "bandar", "fnet_1d", "fnet_5d"])


class HealthTest(unittest.TestCase):
    def test_parse_status(self):
        s = health.parse_status("OK   quotes\nFAIL bandar (exit 4)\nrun at x")
        self.assertEqual(s, {"quotes": True, "bandar": False})

    def test_all_ok_allows_entries(self):
        with tempfile.TemporaryDirectory() as r:
            d = make_snapshot(r, "2026-10-22", ALL_OK, fnet_rows=3)
            h = health.build(d, r)
            self.assertTrue(h["entries_allowed"])
            self.assertEqual(h["pillars"]["broker"], "ok")
            self.assertEqual(h["price_breadth_proxy"], 0.5)

    def test_broker_failure_blocks_and_flags_swallowed_502(self):
        status = ALL_OK.replace("OK   fnet_5d", "FAIL fnet_5d (exit 4)")
        err = '{"error":{"message":"not found: no session with trades in the 10 days"}}'
        with tempfile.TemporaryDirectory() as r:
            make_snapshot(r, "2026-10-23", ALL_OK, fnet_rows=2)
            d = make_snapshot(r, "2026-10-27", status, errs={"fnet_5d": err})
            h = health.build(d, r)
            self.assertFalse(h["entries_allowed"])
            self.assertEqual(h["pillars"]["broker"], "sebagian")
            self.assertIn("502", h["broker_note"])
            self.assertEqual(h["last_good_broker_snapshot"], "2026-10-23")

    def test_low_breadth_blocks(self):
        with tempfile.TemporaryDirectory() as r:
            d = make_snapshot(r, "2026-10-27", ALL_OK, advancing=100, quoted=793, fnet_rows=1)
            h = health.build(d, r)
            self.assertFalse(h["entries_allowed"])
            self.assertTrue(any("breadth" in x for x in h["reasons"]))

    def test_stale_prices_block(self):
        with tempfile.TemporaryDirectory() as r:
            d = make_snapshot(r, "2026-10-27", ALL_OK, as_of="2026-10-26", fnet_rows=1)
            self.assertFalse(health.build(d, r)["entries_allowed"])

    def test_manual_fresh_but_v11_not_ready_still_blocks(self):
        status = ALL_OK.replace("OK   fnet_5d", "FAIL fnet_5d (exit 4)")
        with tempfile.TemporaryDirectory() as top:
            root = os.path.join(top, "snapshots")
            os.makedirs(os.path.join(top, "data", "ringkasan"))
            open(os.path.join(top, "data", "ringkasan", "20261027.csv"), "w").close()
            os.makedirs(os.path.join(top, "signals_v11"))
            with open(os.path.join(top, "signals_v11", "v11_20261027.json"), "w") as f:
                json.dump({"status": "data_kurang (5/14 sesi untuk ATR)"}, f)
            d = make_snapshot(root, "2026-10-27", status, fnet_rows=0)
            h = health.build(d, root)
            self.assertEqual(h["pillars"]["asing_manual"], "ok")
            self.assertFalse(h["entries_allowed"])
            self.assertTrue(any("v1.1" in x for x in h["reasons"]))

    def test_manual_fresh_and_v11_ok_allows(self):
        status = ALL_OK.replace("OK   fnet_5d", "FAIL fnet_5d (exit 4)")
        with tempfile.TemporaryDirectory() as top:
            root = os.path.join(top, "snapshots")
            os.makedirs(os.path.join(top, "data", "ringkasan"))
            open(os.path.join(top, "data", "ringkasan", "20261027.csv"), "w").close()
            os.makedirs(os.path.join(top, "signals_v11"))
            with open(os.path.join(top, "signals_v11", "v11_20261027.json"), "w") as f:
                json.dump({"status": "ok"}, f)
            d = make_snapshot(root, "2026-10-27", status, fnet_rows=0)
            self.assertTrue(health.build(d, root)["entries_allowed"])

    def test_missing_snapshot_does_not_crash(self):
        with tempfile.TemporaryDirectory() as r:
            h = health.build(os.path.join(r, "2026-10-28"), r)
            self.assertFalse(h["entries_allowed"])
            self.assertIsNone(h["last_good_broker_snapshot"])


if __name__ == "__main__":
    unittest.main()
