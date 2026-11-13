import unittest

from forward_test import ft_eval

PLAN = {"symbol": "AAAA", "signal_date": "20261101", "atr": 10.0, "limit": 95.0, "valid_sessions": 3,
        "stop_atr": 2, "target_atr": 4, "hold": 3, "cost": 0.007}


def c(o, h, l, cl):
    return {"AAAA": {"open": o, "high": h, "low": l, "close": cl}}


def days(*rows):
    d = {"20261101": c(100, 101, 99, 100)}
    for i, r in enumerate(rows):
        d[f"2026110{i + 2}"] = c(*r)
    return d


class EvalTest(unittest.TestCase):
    def test_no_data_yet(self):
        self.assertEqual(ft_eval.evaluate(PLAN, days())["status"], "belum_ada_data")

    def test_not_filled_after_valid_sessions(self):
        r = ft_eval.evaluate(PLAN, days((100, 102, 98, 100), (100, 102, 97, 100), (100, 102, 96, 100)))
        self.assertEqual(r["status"], "tidak_terisi")

    def test_waiting_when_window_not_over(self):
        self.assertEqual(ft_eval.evaluate(PLAN, days((100, 102, 98, 100)))["status"], "menunggu_isi")

    def test_fill_at_limit_then_target(self):
        r = ft_eval.evaluate(PLAN, days((100, 101, 94, 99), (99, 140, 98, 130)))
        self.assertEqual((r["status"], r["entry"], r["exit"]), ("kena_target", 95.0, 135.0))

    def test_fill_at_open_when_gap_below_limit(self):
        r = ft_eval.evaluate(PLAN, days((90, 92, 88, 91), (91, 93, 90, 92), (92, 93, 90, 92), (92, 93, 90, 92)))
        self.assertEqual(r["entry"], 90.0)

    def test_open_zero_is_not_a_fill(self):
        r = ft_eval.evaluate(PLAN, days((0, 101, 94, 99)))
        self.assertEqual(r["entry"], 95.0)  # filled by low, never at open 0
        r2 = ft_eval.evaluate(PLAN, days((0, 101, 0, 99), (0, 101, 0, 99), (0, 101, 0, 99)))
        self.assertEqual(r2["status"], "tidak_terisi")

    def test_stop_from_fill_price(self):
        r = ft_eval.evaluate(PLAN, days((100, 101, 94, 99), (99, 100, 74, 80)))
        self.assertEqual((r["status"], r["exit"]), ("kena_stop", 75.0))

    def test_gap_through_stop_exits_at_open(self):
        r = ft_eval.evaluate(PLAN, days((100, 101, 94, 99), (60, 62, 58, 61)))
        self.assertEqual((r["status"], r["exit"]), ("kena_stop", 60.0))

    def test_stop_and_target_same_candle_stop_first(self):
        r = ft_eval.evaluate(PLAN, days((100, 101, 94, 99), (99, 140, 74, 100)))
        self.assertEqual(r["status"], "kena_stop")

    def test_hold_expiry_uses_close_of_last_hold_day(self):
        r = ft_eval.evaluate(PLAN, days((100, 101, 94, 99), (99, 101, 90, 98), (98, 101, 90, 97),
                                        (97, 101, 90, 50)))
        self.assertEqual((r["status"], r["exit"]), ("tahan_habis", 97.0))

    def test_open_position_marked_to_last_close(self):
        r = ft_eval.evaluate(PLAN, days((100, 101, 94, 99), (99, 101, 90, 98)))
        self.assertEqual((r["status"], r["exit"]), ("terbuka", 98.0))

    def test_net_and_r(self):
        r = ft_eval.evaluate(PLAN, days((100, 101, 94, 99), (99, 140, 98, 130)))
        self.assertAlmostEqual(r["net"], 135 / 95 - 1 - 0.007, places=4)
        self.assertAlmostEqual(r["r"], (40 - 0.007 * 95) / 20, places=2)


if __name__ == "__main__":
    unittest.main()
