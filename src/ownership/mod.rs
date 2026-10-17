//! KSEI shareholder register ("Pemegang Saham di Atas 1%") as published
//! monthly by IDX: discovery and download (`discover`), parsing (`ingest`),
//! name normalisation (`normalize`) and the local SQLite store (`db`).
//!
//! Unlike the quote providers, this data is not fetched on demand: a monthly
//! file is imported once and every query runs locally, so the ownership
//! commands work offline after the first import.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::error::{IdxError, Result};
use crate::model::{DiscoveredFile, ImportReport};

pub mod db;
pub mod discover;
pub mod ingest;
pub mod mirror;
pub mod normalize;

pub use db::OwnershipDb;

/// Default location of the SQLite file.
pub fn default_db_path() -> PathBuf {
    let base = if cfg!(windows) {
        dirs::data_local_dir()
    } else {
        dirs::data_dir()
    };
    base.unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("idx")
        .join("ownership.sqlite")
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Parses `bytes` (XLSX or PDF) and stores them. `name` is the file name or
/// URL used for provenance and format detection.
pub fn import_bytes(
    db: &mut OwnershipDb,
    name: &str,
    source: &str,
    bytes: &[u8],
) -> Result<(ImportReport, Vec<String>)> {
    let (parsed, format) = ingest::parse_any(name, bytes)?;
    let report = db.import(&parsed, format, source, &sha256_hex(bytes))?;
    Ok((report, parsed.skipped))
}

pub fn import_file(db: &mut OwnershipDb, path: &Path) -> Result<(ImportReport, Vec<String>)> {
    let bytes = std::fs::read(path)
        .map_err(|e| IdxError::Usage(format!("cannot read {}: {e}", path.display())))?;
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    import_bytes(db, &name, &path.display().to_string(), &bytes)
}

pub async fn import_url(
    db: &mut OwnershipDb,
    url: &str,
    timeout: Duration,
) -> Result<(ImportReport, Vec<String>)> {
    let bytes = discover::fetch_browser(url, timeout).await?;
    import_bytes(db, &discover::basename(url), url, &bytes)
}

/// Downloads and imports every discovered file that is not in the database
/// yet (or only the newest when `latest_only`). Returns the reports in the
/// order imported plus any parser warnings.
pub async fn import_discovered(
    db: &mut OwnershipDb,
    files: &[DiscoveredFile],
    latest_only: bool,
    force: bool,
    timeout: Duration,
) -> Result<(Vec<ImportReport>, Vec<String>)> {
    let have: Vec<chrono::NaiveDate> = db.releases()?.iter().map(|r| r.as_of).collect();
    let mut reports = Vec::new();
    let mut warnings = Vec::new();
    for f in files.iter().filter(|f| force || !have.contains(&f.as_of)) {
        tracing::info!(as_of = %f.as_of, url = %f.url, origin = %f.origin, "importing ownership file");
        let (report, skipped) = if f.origin == mirror::ORIGIN {
            let bytes = mirror::download(&f.url, f.sha256.as_deref(), timeout).await?;
            import_bytes(db, &discover::basename(&f.url), &f.url, &bytes)?
        } else {
            import_url(db, &f.url, timeout).await?
        };
        warnings.extend(skipped.into_iter().map(|s| format!("{}: {s}", f.as_of)));
        reports.push(report);
        if latest_only {
            break;
        }
    }
    Ok((reports, warnings))
}
