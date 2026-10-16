//! Cookie + crumb handshake for Yahoo's authenticated JSON endpoints.
//!
//! 1. `GET https://fc.yahoo.com` answers 404 but sets the `A3` cookie.
//! 2. `GET https://query2.finance.yahoo.com/v1/test/getcrumb` with that cookie
//!    returns a short opaque crumb string.
//!
//! The pair is persisted in the cache for a few hours so a CLI invocation
//! normally costs one request, not three.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{IdxError, Result};
use crate::http::{cookie_pairs, Fetcher};

use super::NAME;

const CACHE_KEY: &str = "yahoo:session";
const TTL: Duration = Duration::from_secs(6 * 3600);
const COOKIE_URL: &str = "https://fc.yahoo.com";
const CRUMB_URL: &str = "https://query2.finance.yahoo.com/v1/test/getcrumb";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub cookie: String,
    pub crumb: String,
    pub created_at: DateTime<Utc>,
}

impl Session {
    pub fn headers(&self) -> Vec<(String, String)> {
        vec![("Cookie".to_string(), self.cookie.clone())]
    }
}

/// Returns a usable session. `None` only when offline and nothing is cached
/// (the caller may still be able to serve a cached quote body).
pub async fn get(fetcher: &Fetcher, force_refresh: bool) -> Result<Option<Session>> {
    if !force_refresh {
        if let Some(s) = fetcher.cache().get_json::<Session>(CACHE_KEY, TTL) {
            return Ok(Some(s));
        }
    }
    if fetcher.is_offline() {
        return Ok(None);
    }

    let cookie_resp = fetcher.get_uncached(COOKIE_URL, &[]).await?;
    let cookie = cookie_pairs(&cookie_resp.header_values("set-cookie"));
    if cookie.is_empty() {
        return Err(IdxError::provider(
            NAME,
            "fc.yahoo.com did not set a cookie",
        ));
    }

    let crumb_resp = fetcher
        .get_uncached(CRUMB_URL, &[("Cookie".to_string(), cookie.clone())])
        .await?;
    let crumb = crumb_resp.body.trim().to_string();
    if crumb_resp.status != 200 || crumb.is_empty() || crumb.len() > 64 || crumb.contains('<') {
        return Err(IdxError::provider(
            NAME,
            format!("could not obtain crumb (HTTP {})", crumb_resp.status),
        ));
    }

    let session = Session {
        cookie,
        crumb,
        created_at: Utc::now(),
    };
    if let Err(e) = fetcher.cache().put_json(CACHE_KEY, &session) {
        tracing::warn!(error = %e, "could not persist yahoo session");
    }
    Ok(Some(session))
}
