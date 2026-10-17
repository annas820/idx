//! SQLite storage for the shareholder register.
//!
//! Schema (one file, WAL mode):
//! - `releases`  one row per imported monthly file, unique on `as_of`
//! - `entities`  one row per normalised holder key, with the display name
//! - `aliases`   every raw spelling seen, pointing at its entity
//! - `holdings`  one row per (release, symbol, entity)
//!
//! Queries never touch the network. Importing the same `as_of` twice replaces
//! the earlier rows for that release.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{IdxError, Result};
use crate::model::{
    Change, ChangeKind, ChangesReport, EntityReport, HoldersReport, Holding, ImportReport, Release,
};

use super::ingest::Parsed;
use super::normalize;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS releases (
  id          INTEGER PRIMARY KEY,
  as_of       TEXT NOT NULL UNIQUE,
  format      TEXT NOT NULL,
  source      TEXT NOT NULL,
  sha256      TEXT NOT NULL,
  row_count   INTEGER NOT NULL,
  imported_at TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS entities (
  id           INTEGER PRIMARY KEY,
  entity_key   TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS aliases (
  raw_name  TEXT PRIMARY KEY,
  entity_id INTEGER NOT NULL REFERENCES entities(id)
);
CREATE TABLE IF NOT EXISTS holdings (
  release_id       INTEGER NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
  symbol           TEXT NOT NULL,
  issuer_name      TEXT,
  entity_id        INTEGER NOT NULL REFERENCES entities(id),
  holder_raw       TEXT NOT NULL,
  investor_type    TEXT,
  local_foreign    TEXT,
  nationality      TEXT,
  domicile         TEXT,
  shares_scripless INTEGER,
  shares_scrip     INTEGER,
  shares_total     INTEGER NOT NULL,
  pct              REAL NOT NULL,
  PRIMARY KEY (release_id, symbol, entity_id)
);
CREATE INDEX IF NOT EXISTS holdings_symbol ON holdings(symbol, release_id);
CREATE INDEX IF NOT EXISTS holdings_entity ON holdings(entity_id, release_id);
";

pub struct OwnershipDb {
    conn: Connection,
    path: PathBuf,
}

fn db_err(e: rusqlite::Error) -> IdxError {
    IdxError::Other(format!("ownership database: {e}"))
}

impl OwnershipDb {
    /// Opens (and creates) the database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).map_err(db_err)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(db_err)?;
        conn.execute_batch(SCHEMA).map_err(db_err)?;
        Ok(OwnershipDb {
            conn,
            path: path.to_path_buf(),
        })
    }

    /// In-memory database for tests.
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory().map_err(db_err)?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .map_err(db_err)?;
        conn.execute_batch(SCHEMA).map_err(db_err)?;
        Ok(OwnershipDb {
            conn,
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stores a parsed file as the release for its `as_of`, replacing any
    /// earlier import of the same date.
    pub fn import(
        &mut self,
        parsed: &Parsed,
        format: &str,
        source: &str,
        sha256: &str,
    ) -> Result<ImportReport> {
        let as_of = parsed.as_of.to_string();
        let tx = self.conn.transaction().map_err(db_err)?;
        let existing: Option<i64> = tx
            .query_row("SELECT id FROM releases WHERE as_of = ?1", [&as_of], |r| {
                r.get(0)
            })
            .optional()
            .map_err(db_err)?;
        if let Some(id) = existing {
            tx.execute("DELETE FROM holdings WHERE release_id = ?1", [id])
                .map_err(db_err)?;
            tx.execute("DELETE FROM releases WHERE id = ?1", [id])
                .map_err(db_err)?;
        }
        let imported_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        tx.execute(
            "INSERT INTO releases (as_of, format, source, sha256, row_count, imported_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![as_of, format, source, sha256, parsed.rows.len() as i64, imported_at],
        )
        .map_err(db_err)?;
        let release_id = tx.last_insert_rowid();

        let mut entity_cache: HashMap<String, i64> = HashMap::new();
        let mut entities_touched = std::collections::HashSet::new();
        {
            let mut find_alias = tx
                .prepare("SELECT entity_id FROM aliases WHERE raw_name = ?1")
                .map_err(db_err)?;
            let mut find_entity = tx
                .prepare("SELECT id FROM entities WHERE entity_key = ?1")
                .map_err(db_err)?;
            let mut insert_entity = tx
                .prepare("INSERT INTO entities (entity_key, display_name) VALUES (?1, ?2)")
                .map_err(db_err)?;
            let mut insert_alias = tx
                .prepare("INSERT OR IGNORE INTO aliases (raw_name, entity_id) VALUES (?1, ?2)")
                .map_err(db_err)?;
            let mut insert_holding = tx
                .prepare(
                    "INSERT OR REPLACE INTO holdings (release_id, symbol, issuer_name, entity_id, holder_raw,
                     investor_type, local_foreign, nationality, domicile, shares_scripless, shares_scrip,
                     shares_total, pct) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
                )
                .map_err(db_err)?;

            for r in &parsed.rows {
                let key = normalize::entity_key(&r.holder_raw);
                let entity_id = match entity_cache.get(&key) {
                    Some(id) => *id,
                    None => {
                        let by_alias: Option<i64> = find_alias
                            .query_row([&r.holder_raw], |row| row.get(0))
                            .optional()
                            .map_err(db_err)?;
                        let id = match by_alias {
                            Some(id) => id,
                            None => {
                                let by_key: Option<i64> = find_entity
                                    .query_row([&key], |row| row.get(0))
                                    .optional()
                                    .map_err(db_err)?;
                                match by_key {
                                    Some(id) => id,
                                    None => {
                                        insert_entity
                                            .execute(params![
                                                key,
                                                normalize::display_name(&r.holder_raw)
                                            ])
                                            .map_err(db_err)?;
                                        tx.last_insert_rowid()
                                    }
                                }
                            }
                        };
                        entity_cache.insert(key.clone(), id);
                        id
                    }
                };
                insert_alias
                    .execute(params![r.holder_raw, entity_id])
                    .map_err(db_err)?;
                entities_touched.insert(entity_id);
                insert_holding
                    .execute(params![
                        release_id,
                        r.symbol,
                        r.issuer_name,
                        entity_id,
                        r.holder_raw,
                        r.investor_type,
                        r.local_foreign,
                        r.nationality,
                        r.domicile,
                        r.shares_scripless,
                        r.shares_scrip,
                        r.shares_total,
                        r.pct,
                    ])
                    .map_err(db_err)?;
            }
        }
        tx.commit().map_err(db_err)?;
        Ok(ImportReport {
            release: Release {
                as_of: parsed.as_of,
                format: format.to_string(),
                source: source.to_string(),
                sha256: sha256.to_string(),
                rows: parsed.rows.len() as u64,
                issuers: parsed.issuers() as u64,
                entities: entities_touched.len() as u64,
                imported_at,
            },
            replaced: existing.is_some(),
            database: self.path.display().to_string(),
        })
    }

    /// Every issuer code seen in any release, with the latest name on file.
    pub fn issuers(&self) -> Result<Vec<(String, Option<String>)>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT symbol, MAX(issuer_name) FROM holdings GROUP BY symbol ORDER BY symbol",
            )
            .map_err(db_err)?;
        let rows = st
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .map_err(db_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    pub fn releases(&self) -> Result<Vec<Release>> {
        let mut st = self
            .conn
            .prepare(
                "SELECT r.as_of, r.format, r.source, r.sha256, r.row_count, r.imported_at,
                        (SELECT COUNT(DISTINCT symbol) FROM holdings h WHERE h.release_id = r.id),
                        (SELECT COUNT(DISTINCT entity_id) FROM holdings h WHERE h.release_id = r.id)
                 FROM releases r ORDER BY r.as_of",
            )
            .map_err(db_err)?;
        let rows = st
            .query_map([], |row| {
                Ok(Release {
                    as_of: parse_date(&row.get::<_, String>(0)?),
                    format: row.get(1)?,
                    source: row.get(2)?,
                    sha256: row.get(3)?,
                    rows: row.get::<_, i64>(4)? as u64,
                    imported_at: row.get(5)?,
                    issuers: row.get::<_, i64>(6)? as u64,
                    entities: row.get::<_, i64>(7)? as u64,
                })
            })
            .map_err(db_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    /// Release id for `as_of` (exact), or the latest when `None`.
    fn release_for(&self, as_of: Option<NaiveDate>) -> Result<(i64, NaiveDate)> {
        let row: Option<(i64, String)> = match as_of {
            Some(d) => self
                .conn
                .query_row(
                    "SELECT id, as_of FROM releases WHERE as_of = ?1",
                    [d.to_string()],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db_err)?,
            None => self
                .conn
                .query_row(
                    "SELECT id, as_of FROM releases ORDER BY as_of DESC LIMIT 1",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()
                .map_err(db_err)?,
        };
        match row {
            Some((id, d)) => Ok((id, parse_date(&d))),
            None => {
                let available: Vec<String> = self
                    .releases()?
                    .iter()
                    .map(|r| r.as_of.to_string())
                    .collect();
                Err(IdxError::NotFound(if available.is_empty() {
                    "no ownership data imported yet; run `idx ownership import --latest` or `--file <xlsx>`".into()
                } else {
                    format!(
                        "no ownership release for {}; available: {}",
                        as_of
                            .map(|d| d.to_string())
                            .unwrap_or_else(|| "latest".into()),
                        available.join(", ")
                    )
                }))
            }
        }
    }

    fn holdings_where(
        &self,
        release_id: i64,
        as_of: NaiveDate,
        clause: &str,
        arg: &str,
    ) -> Result<Vec<Holding>> {
        let sql = format!(
            "SELECT h.symbol, h.issuer_name, e.display_name, h.holder_raw, e.entity_key, h.investor_type,
                    h.local_foreign, h.nationality, h.domicile, h.shares_scripless, h.shares_scrip,
                    h.shares_total, h.pct
             FROM holdings h JOIN entities e ON e.id = h.entity_id
             WHERE h.release_id = ?1 AND {clause}
             ORDER BY h.pct DESC, h.symbol"
        );
        let mut st = self.conn.prepare(&sql).map_err(db_err)?;
        let rows = st
            .query_map(params![release_id, arg], |row| {
                Ok(Holding {
                    as_of,
                    symbol: row.get(0)?,
                    issuer_name: row.get(1)?,
                    holder: row.get(2)?,
                    holder_raw: row.get(3)?,
                    entity_key: row.get(4)?,
                    investor_type: row.get(5)?,
                    local_foreign: row.get(6)?,
                    nationality: row.get(7)?,
                    domicile: row.get(8)?,
                    shares_scripless: row.get(9)?,
                    shares_scrip: row.get(10)?,
                    shares_total: row.get(11)?,
                    pct: row.get(12)?,
                })
            })
            .map_err(db_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(db_err)?;
        Ok(rows)
    }

    /// Holders above 1 % of `symbol` in the release for `as_of` (latest if `None`).
    pub fn holders(&self, symbol: &str, as_of: Option<NaiveDate>) -> Result<HoldersReport> {
        let (release_id, date) = self.release_for(as_of)?;
        let holders =
            self.holdings_where(release_id, date, "h.symbol = ?2", &symbol.to_uppercase())?;
        if holders.is_empty() {
            return Err(IdxError::NotFound(format!(
                "no holders above 1% recorded for {symbol} as of {date}"
            )));
        }
        Ok(HoldersReport {
            symbol: symbol.to_uppercase(),
            issuer_name: holders.iter().find_map(|h| h.issuer_name.clone()),
            as_of: date,
            listed_pct: holders.iter().map(|h| h.pct).sum(),
            holders,
        })
    }

    /// Every position of holders whose normalised name contains `query`.
    pub fn entity(&self, query: &str, as_of: Option<NaiveDate>) -> Result<EntityReport> {
        let (release_id, date) = self.release_for(as_of)?;
        let key = normalize::entity_key(query);
        let pattern = format!("%{key}%");
        let holdings = self.holdings_where(release_id, date, "e.entity_key LIKE ?2", &pattern)?;
        if holdings.is_empty() {
            return Err(IdxError::NotFound(format!(
                "no holder matching {query:?} as of {date}"
            )));
        }
        let mut entities: Vec<String> = holdings.iter().map(|h| h.holder.clone()).collect();
        entities.sort();
        entities.dedup();
        Ok(EntityReport {
            query: query.to_string(),
            as_of: date,
            entities,
            holdings,
        })
    }

    /// Movements between two releases.
    pub fn changes(
        &self,
        from: NaiveDate,
        to: NaiveDate,
        symbol: Option<&str>,
        min_pct_change: f64,
        limit: Option<usize>,
    ) -> Result<ChangesReport> {
        if from >= to {
            return Err(IdxError::Usage("--from must be earlier than --to".into()));
        }
        let (from_id, _) = self.release_for(Some(from))?;
        let (to_id, _) = self.release_for(Some(to))?;
        let symbol = symbol.map(str::to_uppercase);

        let sql =
            "SELECT h.symbol, e.display_name, e.entity_key, h.investor_type, h.shares_total, h.pct
                   FROM holdings h JOIN entities e ON e.id = h.entity_id
                   WHERE h.release_id = ?1 AND (?2 IS NULL OR h.symbol = ?2)";
        let load = |release_id: i64| -> Result<Positions> {
            let mut st = self.conn.prepare(sql).map_err(db_err)?;
            let rows = st
                .query_map(params![release_id, symbol], |row| {
                    Ok((
                        (row.get::<_, String>(0)?, row.get::<_, String>(2)?),
                        (
                            row.get::<_, String>(1)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, f64>(5)?,
                        ),
                    ))
                })
                .map_err(db_err)?
                .collect::<std::result::Result<BTreeMap<_, _>, _>>()
                .map_err(db_err)?;
            Ok(rows)
        };
        let before = load(from_id)?;
        let after = load(to_id)?;

        let mut changes: Vec<Change> = Vec::new();
        for ((sym, key), (name, itype, shares_to, pct_to)) in &after {
            match before.get(&(sym.clone(), key.clone())) {
                Some((_, _, shares_from, pct_from)) => {
                    let pct_change = pct_to - pct_from;
                    if (pct_change).abs() < 1e-9 && shares_to == shares_from {
                        continue;
                    }
                    changes.push(Change {
                        symbol: sym.clone(),
                        holder: name.clone(),
                        entity_key: key.clone(),
                        investor_type: itype.clone(),
                        kind: ChangeKind::Changed,
                        pct_from: Some(*pct_from),
                        pct_to: Some(*pct_to),
                        pct_change: round4(pct_change),
                        shares_from: Some(*shares_from),
                        shares_to: Some(*shares_to),
                        shares_change: shares_to - shares_from,
                    });
                }
                None => changes.push(Change {
                    symbol: sym.clone(),
                    holder: name.clone(),
                    entity_key: key.clone(),
                    investor_type: itype.clone(),
                    kind: ChangeKind::New,
                    pct_from: None,
                    pct_to: Some(*pct_to),
                    pct_change: round4(*pct_to),
                    shares_from: None,
                    shares_to: Some(*shares_to),
                    shares_change: *shares_to,
                }),
            }
        }
        for ((sym, key), (name, itype, shares_from, pct_from)) in &before {
            if !after.contains_key(&(sym.clone(), key.clone())) {
                changes.push(Change {
                    symbol: sym.clone(),
                    holder: name.clone(),
                    entity_key: key.clone(),
                    investor_type: itype.clone(),
                    kind: ChangeKind::Exited,
                    pct_from: Some(*pct_from),
                    pct_to: None,
                    pct_change: round4(-pct_from),
                    shares_from: Some(*shares_from),
                    shares_to: None,
                    shares_change: -shares_from,
                });
            }
        }
        changes.retain(|c| c.pct_change.abs() + 1e-12 >= min_pct_change);
        changes.sort_by(|a, b| {
            b.pct_change
                .abs()
                .partial_cmp(&a.pct_change.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.symbol.cmp(&b.symbol))
        });
        let total = changes.len();
        if let Some(n) = limit {
            changes.truncate(n);
        }
        Ok(ChangesReport {
            from,
            to,
            symbol,
            min_pct_change,
            total_matches: total,
            changes,
        })
    }
}

/// (symbol, entity_key) -> (display name, investor type, shares, pct).
type Positions = BTreeMap<(String, String), (String, Option<String>, i64, f64)>;

fn round4(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}

fn parse_date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ownership::ingest::parse_xlsx;

    const XLSX: &[u8] =
        include_bytes!("../../tests/fixtures/ownership/above1_20260831_excerpt.xlsx");
    const XLSX_PREV: &[u8] =
        include_bytes!("../../tests/fixtures/ownership/above1_20260731_synthetic.xlsx");

    fn loaded() -> OwnershipDb {
        let mut db = OwnershipDb::in_memory().unwrap();
        let prev = parse_xlsx(XLSX_PREV).unwrap();
        db.import(&prev, "xlsx", "prev.xlsx", "aaa").unwrap();
        let cur = parse_xlsx(XLSX).unwrap();
        db.import(&cur, "xlsx", "cur.xlsx", "bbb").unwrap();
        db
    }

    #[test]
    fn import_and_releases() {
        let db = loaded();
        let r = db.releases().unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].as_of, NaiveDate::from_ymd_opt(2026, 8, 31).unwrap());
        assert!(r[1].rows > 10);
        assert!(r[1].entities > 5);
    }

    #[test]
    fn reimport_replaces() {
        let mut db = loaded();
        let cur = parse_xlsx(XLSX).unwrap();
        let rep = db.import(&cur, "xlsx", "again.xlsx", "ccc").unwrap();
        assert!(rep.replaced);
        assert_eq!(db.releases().unwrap().len(), 2);
    }

    #[test]
    fn holders_latest_and_dated() {
        let db = loaded();
        let h = db.holders("aadi", None).unwrap();
        assert_eq!(h.symbol, "AADI");
        assert_eq!(h.as_of, NaiveDate::from_ymd_opt(2026, 8, 31).unwrap());
        assert!(h.holders[0].pct >= h.holders[1].pct, "sorted by pct");
        assert!(h.listed_pct > 50.0);
        assert_eq!(h.holders[0].holder, "ADARO STRATEGIC INVESTMENTS");
        let err = db.holders("ZZZZ", None).unwrap_err();
        assert_eq!(err.exit_code(), 4);
        let err = db
            .holders("AADI", NaiveDate::from_ymd_opt(2020, 1, 1))
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("available: 2026-07-31, 2026-08-31"),
            "{err}"
        );
    }

    #[test]
    fn entity_search_groups_spellings() {
        let db = loaded();
        let e = db.entity("alamtri resources", None).unwrap();
        assert!(!e.holdings.is_empty());
        assert!(e.entities.iter().all(|n| n.contains("ALAMTRI")));
        assert!(db.entity("no such holder", None).is_err());
    }

    #[test]
    fn changes_between_months() {
        let db = loaded();
        let c = db
            .changes(
                NaiveDate::from_ymd_opt(2026, 7, 31).unwrap(),
                NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
                None,
                0.0,
                None,
            )
            .unwrap();
        assert!(c.total_matches >= 3, "{c:?}");
        let kinds: Vec<ChangeKind> = c.changes.iter().map(|x| x.kind).collect();
        assert!(kinds.contains(&ChangeKind::Changed));
        assert!(kinds.contains(&ChangeKind::New));
        assert!(kinds.contains(&ChangeKind::Exited));
        assert!(c.changes[0].pct_change.abs() >= c.changes[1].pct_change.abs());
        let only = db
            .changes(
                NaiveDate::from_ymd_opt(2026, 7, 31).unwrap(),
                NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
                Some("aadi"),
                0.5,
                Some(1),
            )
            .unwrap();
        assert_eq!(only.changes.len(), 1);
        assert!(only.changes.iter().all(|x| x.symbol == "AADI"));
        assert!(db
            .changes(
                NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
                NaiveDate::from_ymd_opt(2026, 7, 31).unwrap(),
                None,
                0.0,
                None
            )
            .is_err());
    }
}
