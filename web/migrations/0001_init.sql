-- idx web terminal: D1 schema.
-- Writes are kept to a handful per minute so the free D1 tier is enough:
-- the whole quote board is one JSON row, the index tape is one row per minute.

-- Key/value blobs: quotes board, Yahoo session, market summary.
CREATE TABLE IF NOT EXISTS kv (
  k          TEXT PRIMARY KEY,
  v          TEXT NOT NULL,
  updated_at INTEGER NOT NULL            -- unix seconds
);

-- Minute tape of the composite index (^JKSE) and a few headline names.
CREATE TABLE IF NOT EXISTS ticks (
  symbol TEXT NOT NULL,
  ts     INTEGER NOT NULL,               -- unix seconds, minute aligned
  price  REAL NOT NULL,
  PRIMARY KEY (symbol, ts)
);

-- On-demand response cache (history, fundamentals, news, ...), TTL per row.
CREATE TABLE IF NOT EXISTS cache (
  k          TEXT PRIMARY KEY,
  v          TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  fetched_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS cache_expiry ON cache(expires_at);

-- KSEI >1% register, synced from a maintainer's local idx database
-- (scripts/ownership-sync.mjs). Same shape as the CLI tables, flattened.
CREATE TABLE IF NOT EXISTS own_releases (
  as_of   TEXT PRIMARY KEY,
  format  TEXT NOT NULL,
  source  TEXT NOT NULL,
  rows    INTEGER NOT NULL,
  synced_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS own_holdings (
  as_of         TEXT NOT NULL,
  symbol        TEXT NOT NULL,
  issuer_name   TEXT,
  entity_key    TEXT NOT NULL,
  holder        TEXT NOT NULL,
  holder_raw    TEXT NOT NULL,
  investor_type TEXT,
  local_foreign TEXT,
  shares_total  INTEGER NOT NULL,
  pct           REAL NOT NULL,
  PRIMARY KEY (as_of, symbol, entity_key)
);
CREATE INDEX IF NOT EXISTS own_symbol ON own_holdings(symbol, as_of);
CREATE INDEX IF NOT EXISTS own_entity ON own_holdings(entity_key, as_of);
