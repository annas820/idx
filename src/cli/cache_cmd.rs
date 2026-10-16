//! `idx cache ...`

use clap::Subcommand;

use crate::cache::Cache;
use crate::error::Result;
use crate::output::{self, OutputFormat};

use super::GlobalOpts;

#[derive(Debug, Subcommand)]
pub enum CacheCmd {
    /// Show cache location, file count and size.
    Stats,
    /// Delete every cached response (and the Yahoo session).
    Clear,
    /// Print the cache directory.
    Path,
}

pub fn run(global: &GlobalOpts, cmd: CacheCmd) -> Result<()> {
    let settings = super::resolve_settings(global)?;
    let cache = Cache::new(&settings.cache_dir);
    let json = settings.output == OutputFormat::Json;
    match cmd {
        CacheCmd::Stats => {
            let (files, bytes) = cache.stats()?;
            if json {
                output::print_stdout(&output::json::to_pretty(&serde_json::json!({
                    "dir": cache.dir().display().to_string(),
                    "files": files,
                    "bytes": bytes,
                }))?)
            } else {
                let size = if bytes >= 1 << 20 {
                    format!("{:.1} MiB", bytes as f64 / (1u64 << 20) as f64)
                } else if bytes >= 1 << 10 {
                    format!("{:.1} KiB", bytes as f64 / 1024.0)
                } else {
                    format!("{bytes} B")
                };
                output::print_stdout(&format!(
                    "dir   : {}\nfiles : {}\nsize  : {}\n",
                    cache.dir().display(),
                    files,
                    size
                ))
            }
        }
        CacheCmd::Clear => {
            let n = cache.clear()?;
            if json {
                output::print_stdout(&output::json::to_pretty(
                    &serde_json::json!({ "removed": n }),
                )?)
            } else {
                output::print_stderr(&format!(
                    "removed {n} cached file(s) from {}",
                    cache.dir().display()
                ));
                Ok(())
            }
        }
        CacheCmd::Path => output::print_stdout(&format!("{}\n", cache.dir().display())),
    }
}
