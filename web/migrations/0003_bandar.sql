-- Nightly bandarmology scan (src/bandar-scan.js): one row per stock and
-- period for the session's most traded stocks; the newest two sessions are kept.
CREATE TABLE IF NOT EXISTS bandar_scan (
  session TEXT NOT NULL,                 -- last session of every range, YYYY-MM-DD (WIB)
  period  TEXT NOT NULL,                 -- 1d | 1w | 1m | 3m
  symbol  TEXT NOT NULL,
  v       TEXT NOT NULL,                 -- {summary, price, chg} as JSON
  PRIMARY KEY (session, period, symbol)
);
