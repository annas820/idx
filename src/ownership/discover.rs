//! Finding and downloading the monthly files on idx.co.id.
//!
//! The "Data Kepemilikan Saham" page is a Nuxt app whose payload embeds
//! `{Description:"Pemegang Saham di Atas 1% per 31 Agustus 2026",
//! Prospectus:"https://www.idx.co.id/Media/...xlsx", ...}`
//! entries. We parse those pairs out of the HTML without a JS engine.
//!
//! idx.co.id sits behind Cloudflare bot management: plain HTTP clients get a
//! 403 challenge page. `fetch_browser` tries a browser-shaped request first,
//! then shells out to a `curl-impersonate` binary when one is installed.
//! When both fail, `discover` falls back to the GitHub mirror that CI keeps
//! in sync (see `mirror.rs`), so `idx ownership import --latest` works
//! without any extra tooling. `file://` URLs read a saved page or file.

use std::path::Path;
use std::time::Duration;

use chrono::NaiveDate;

use crate::error::{IdxError, Result};
use crate::model::DiscoveredFile;

use super::ingest::parse_indonesian_date;

pub const PAGE_URL: &str = "https://www.idx.co.id/id/perusahaan-tercatat/data-kepemilikan-saham/";
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
/// Binaries tried in order; `IDX_CURL_IMPERSONATE` overrides the list.
const IMPERSONATE_BINARIES: &[&str] = &[
    "curl_chrome131",
    "curl_chrome124",
    "curl_chrome116",
    "curl-impersonate-chrome",
    "curl-impersonate",
];

pub const MANUAL_HINT: &str = "idx.co.id is behind Cloudflare bot protection. Options: install curl-impersonate (https://github.com/lexiforest/curl-impersonate) and put curl_chrome131 on PATH, or open https://www.idx.co.id/id/perusahaan-tercatat/data-kepemilikan-saham/ in a browser, download the \"Pemegang Saham di Atas 1%\" workbook and run `idx ownership import --file <path>`";

fn unescape_js(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Ok(code) = u32::from_str_radix(&hex, 16) {
                    if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                    }
                }
            }
            Some('/') => out.push('/'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Reads a JS string literal starting at `s` (which begins with a quote).
fn js_string(s: &str) -> Option<(String, usize)> {
    let quote = s.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let mut escaped = false;
    for (i, c) in s.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
        } else if c == quote {
            return Some((unescape_js(&s[1..i]), i + 1));
        }
    }
    None
}

/// `per 31 Agustus 2026` -> date.
pub fn as_of_from_title(title: &str) -> Option<NaiveDate> {
    let idx = title.to_lowercase().rfind(" per ")?;
    let tail = title[idx + 5..].trim().trim_end_matches('.');
    parse_indonesian_date(tail).or_else(|| super::ingest::parse_text_date(tail))
}

/// Every `Description`/`Prospectus` pair in the page, newest first. Only the
/// above-1 % register is kept; the above-5 % and investor-type workbooks live
/// on the same page but have other layouts.
pub fn parse_page(html: &str) -> Vec<DiscoveredFile> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(pos) = rest.find("Description:") {
        rest = &rest[pos + "Description:".len()..];
        let Some((title, used)) = js_string(rest) else {
            continue;
        };
        let after = &rest[used..];
        // The URL follows within the same object literal.
        let Some(ppos) = after.find("Prospectus:") else {
            continue;
        };
        if ppos > 400 {
            continue;
        }
        let Some((url, _)) = js_string(&after[ppos + "Prospectus:".len()..]) else {
            continue;
        };
        let lower = title.to_lowercase();
        let is_above1 = lower.contains("di atas 1%")
            || lower.contains("di atas 1 %")
            || lower.contains("diatas 1%");
        if !is_above1 {
            continue;
        }
        let Some(as_of) = as_of_from_title(&title) else {
            continue;
        };
        let format = if url.to_lowercase().ends_with(".pdf") {
            "pdf"
        } else {
            "xlsx"
        };
        out.push(DiscoveredFile {
            as_of,
            title: title.trim().to_string(),
            url,
            format: format.to_string(),
            origin: "idx.co.id".to_string(),
            sha256: None,
            source_url: None,
        });
    }
    out.sort_by_key(|a| std::cmp::Reverse(a.as_of));
    out.dedup_by(|a, b| a.as_of == b.as_of);
    out
}

fn looks_like_challenge(status: u16, body: &[u8]) -> bool {
    if status == 403 || status == 503 {
        return true;
    }
    let head = String::from_utf8_lossy(&body[..body.len().min(4096)]).to_lowercase();
    head.contains("just a moment") || head.contains("cf-chl") || head.contains("challenge-platform")
}

async fn fetch_plain(url: &str, timeout: Duration) -> Result<(u16, Vec<u8>)> {
    let client = reqwest::Client::builder()
        .user_agent(UA)
        .timeout(timeout)
        .gzip(true)
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;
    let resp = client
        .get(url)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header("Accept-Language", "id-ID,id;q=0.9,en-US;q=0.8,en;q=0.7")
        .header("Referer", PAGE_URL)
        .header("Sec-Fetch-Dest", "document")
        .header("Sec-Fetch-Mode", "navigate")
        .header("Sec-Fetch-Site", "same-origin")
        .header("Upgrade-Insecure-Requests", "1")
        .send()
        .await?;
    let status = resp.status().as_u16();
    let bytes = resp.bytes().await?.to_vec();
    Ok((status, bytes))
}

fn impersonate_candidates() -> Vec<String> {
    if let Ok(custom) = std::env::var("IDX_CURL_IMPERSONATE") {
        if !custom.trim().is_empty() {
            return vec![custom];
        }
    }
    IMPERSONATE_BINARIES.iter().map(|s| s.to_string()).collect()
}

fn fetch_impersonated(url: &str, timeout: Duration) -> Option<Result<(u16, Vec<u8>)>> {
    for bin in impersonate_candidates() {
        let tmp = std::env::temp_dir().join(format!("idx-dl-{}.bin", std::process::id()));
        let out = std::process::Command::new(&bin)
            .args([
                "-sS",
                "-L",
                "--max-time",
                &timeout.as_secs().to_string(),
                "-H",
                &format!("Referer: {PAGE_URL}"),
                "-o",
            ])
            .arg(&tmp)
            .args(["-w", "%{http_code}"])
            .arg(url)
            .output();
        match out {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Some(Err(IdxError::Network(format!("{bin}: {e}")))),
            Ok(o) => {
                let code: u16 = String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .parse()
                    .unwrap_or(0);
                let body = std::fs::read(&tmp).unwrap_or_default();
                let _ = std::fs::remove_file(&tmp);
                tracing::info!(binary = %bin, status = code, "fetched through curl-impersonate");
                if !o.status.success() && code == 0 {
                    return Some(Err(IdxError::Network(format!(
                        "{bin} failed: {}",
                        String::from_utf8_lossy(&o.stderr).trim()
                    ))));
                }
                return Some(Ok((code, body)));
            }
        }
    }
    None
}

/// Downloads `url` as a browser would. See the module docs for the strategy.
pub async fn fetch_browser(url: &str, timeout: Duration) -> Result<Vec<u8>> {
    if let Some(path) = url.strip_prefix("file://") {
        let path = if cfg!(windows) {
            path.trim_start_matches('/')
        } else {
            path
        };
        return std::fs::read(path)
            .map_err(|e| IdxError::Usage(format!("cannot read {path}: {e}")));
    }
    let (status, body) = fetch_plain(url, timeout).await?;
    if (200..300).contains(&status) && !looks_like_challenge(status, &body) {
        return Ok(body);
    }
    tracing::info!(status, "plain request was blocked, trying curl-impersonate");
    match fetch_impersonated(url, timeout) {
        Some(Ok((code, body))) if (200..300).contains(&code) && !looks_like_challenge(code, &body) => {
            Ok(body)
        }
        Some(Ok((code, _))) => Err(IdxError::Network(format!(
            "idx.co.id answered HTTP {code} even through curl-impersonate. {MANUAL_HINT}"
        ))),
        Some(Err(e)) => Err(e),
        None => Err(IdxError::Network(format!(
            "idx.co.id answered HTTP {status} to a plain request and no curl-impersonate binary was found. {MANUAL_HINT}"
        ))),
    }
}

/// Fetches the listing page and returns the above-1 % files, newest first.
/// Returns the files plus notes (for example that the mirror was used).
///
/// `IDX_OWNERSHIP_SOURCE`: `auto` (default: idx.co.id, then the mirror),
/// `idx` (never the mirror) or `mirror` (skip idx.co.id).
pub async fn discover(timeout: Duration) -> Result<(Vec<DiscoveredFile>, Vec<String>)> {
    let source = std::env::var("IDX_OWNERSHIP_SOURCE")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if source == "mirror" {
        let base = super::mirror::base().ok_or_else(|| {
            IdxError::Config("IDX_OWNERSHIP_SOURCE=mirror but IDX_OWNERSHIP_MIRROR=off".into())
        })?;
        let index = super::mirror::index(&base, timeout).await?;
        return Ok((
            super::mirror::to_discovered(&base, &index),
            vec![format!(
                "IDX_OWNERSHIP_SOURCE=mirror: listing from {base} (same IDX files, SHA-256 checked on download)"
            )],
        ));
    }
    let explicit_page = std::env::var("IDX_OWNERSHIP_PAGE_URL").ok();
    let page_url = explicit_page
        .clone()
        .unwrap_or_else(|| PAGE_URL.to_string());
    let direct = match fetch_browser(&page_url, timeout).await {
        Ok(bytes) => {
            let files = parse_page(&String::from_utf8_lossy(&bytes));
            if files.is_empty() {
                Err(IdxError::provider(
                    "idx",
                    "the Data Kepemilikan Saham page had no \"Pemegang Saham di Atas 1%\" entries; the page layout may have changed",
                ))
            } else {
                Ok(files)
            }
        }
        Err(e) => Err(e),
    };
    let err = match direct {
        Ok(files) => return Ok((files, vec![])),
        Err(e) => e,
    };
    // A page the caller pointed at explicitly is not second-guessed.
    let Some(base) = super::mirror::base().filter(|_| explicit_page.is_none() && source != "idx")
    else {
        return Err(err);
    };
    tracing::info!(error = %err, mirror = %base, "idx.co.id unavailable, using the GitHub mirror");
    match super::mirror::index(&base, timeout).await {
        Ok(index) => {
            let files = super::mirror::to_discovered(&base, &index);
            if files.is_empty() {
                return Err(err);
            }
            let reason =
                if err.to_string().contains("Cloudflare") || err.to_string().contains("HTTP 403") {
                    "idx.co.id refused this client (Cloudflare)".to_string()
                } else {
                    format!("idx.co.id unavailable ({})", short(&err.to_string()))
                };
            Ok((
                files,
                vec![format!(
                    "{reason}; using the GitHub mirror {base}, which holds the same IDX files (SHA-256 checked on download)"
                )],
            ))
        }
        Err(me) => Err(IdxError::Network(format!(
            "{err}. The GitHub mirror also failed: {me}"
        ))),
    }
}

fn short(s: &str) -> String {
    let first = s.split(". ").next().unwrap_or(s);
    if first.chars().count() > 120 {
        format!("{}…", first.chars().take(120).collect::<String>())
    } else {
        first.to_string()
    }
}

/// File name component of a URL or path, for provenance.
pub fn basename(source: &str) -> String {
    Path::new(source.split('?').next().unwrap_or(source))
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| source.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = include_str!("../../tests/fixtures/ownership/idx_page_excerpt.html");

    #[test]
    fn page_lists_above1_files_newest_first() {
        let files = parse_page(PAGE);
        let dates: Vec<String> = files.iter().map(|f| f.as_of.to_string()).collect();
        assert_eq!(dates, ["2026-08-31", "2026-07-31", "2026-02-27"]);
        assert_eq!(
            files[0].url,
            "https://www.idx.co.id/Media/fahlw1o2/peng-2026-08-00017-satu-persen.xlsx"
        );
        assert_eq!(files[0].format, "xlsx");
        assert!(files[0]
            .title
            .starts_with("Pemegang Saham di Atas 1% per 31 Agustus"));
        assert!(
            !files.iter().any(|f| f.url.contains("lima-persen")),
            "above-5% files are filtered out"
        );
    }

    #[test]
    fn title_dates_and_js_strings() {
        assert_eq!(
            as_of_from_title("Pemegang Saham di Atas 1% per 31 Juli 2026"),
            Some(NaiveDate::from_ymd_opt(2026, 7, 31).unwrap())
        );
        assert_eq!(
            unescape_js("https:\\u002F\\u002Fx.y\\u002Fa.xlsx"),
            "https://x.y/a.xlsx"
        );
        assert_eq!(js_string("\"a\\\"b\" rest").unwrap().0, "a\"b");
        assert_eq!(basename("https://x/y/z.xlsx?x=1"), "z.xlsx");
    }

    #[tokio::test]
    async fn file_urls_read_saved_pages() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/ownership/idx_page_excerpt.html"
        )
        .replace('\\', "/");
        let bytes = fetch_browser(&format!("file://{path}"), Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(parse_page(&String::from_utf8_lossy(&bytes)).len(), 3);
        assert!(
            fetch_browser("file:///no/such/page.html", Duration::from_secs(5))
                .await
                .is_err()
        );
    }

    #[test]
    fn challenge_detection() {
        assert!(looks_like_challenge(403, b""));
        assert!(looks_like_challenge(
            200,
            b"<title>Just a moment...</title>"
        ));
        assert!(!looks_like_challenge(200, b"PK\x03\x04"));
    }
}
