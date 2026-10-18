//! Interactive Stockbit login: launch a Chromium-family browser the user signs
//! into, and capture the session token off the network over the Chrome
//! DevTools Protocol. The human does the login (password, OTP, captcha); every
//! step after — capture, storage, refresh — is automatic.
//!
//! The token can arrive two ways, both handled:
//! - a fresh login issues a `refresh` JWT on its `/login/*` or `/auth/*`
//!   response, which `Fetch.requestPaused` intercepts;
//! - an already-signed-in browser lands straight in the app with no login
//!   response, but the credential sits in the `credentialStorage` cookie, which
//!   the fallback reads with `Storage.getCookies`.
//!
//! The token-recognition rules (`tokenUrlAllowed`, the refresh/cookie readers)
//! live in `auth.rs` and `super`, and are unit-tested; this module is the CDP
//! orchestration around them, which needs a real browser and is covered by
//! `scripts/smoke.sh`, not `cargo test`.

use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::sync::{oneshot, Mutex};

use crate::error::{IdxError, Result};

use super::auth::{session_from_credential_blob, Session};

/// How a captured session was obtained, for the success message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Captured {
    /// Intercepted on a fresh login response.
    Login,
    /// Read from the already-signed-in browser's cookie.
    AlreadySignedIn,
}

pub struct LoginOptions {
    /// Browser executable; discovered when `None`.
    pub browser: Option<PathBuf>,
    /// Persistent profile directory, so a second login need not re-type.
    pub profile_dir: PathBuf,
    /// Overall ceiling on the login.
    pub timeout: Duration,
}

/// The Stockbit web login page the browser opens.
const LOGIN_URL: &str = "https://stockbit.com/login";

/// Whether a response URL may carry the main-session refresh token. Ported from
/// stockbit-mcp's `tokenUrlAllowed`: exodus login/auth responses only; never
/// the securities or e-IPO partner domains, whose tokens exodus rejects.
pub fn token_url_allowed(url: &str) -> bool {
    // Parse the host rather than substring-match it: a path component such as
    // `https://evil.test/exodus.stockbit.com/login/` contains the host name and
    // is not the host — exactly the confusion this exists to prevent.
    let Some((host, path)) = split_host_path(url) else {
        return false;
    };
    if host != "exodus.stockbit.com" {
        return false;
    }
    let p = path.to_ascii_lowercase();
    if p.contains("verification") || p.contains("/otp") || p.contains("/partner/") {
        return false;
    }
    p.starts_with("/login/")
        || p.starts_with("/auth/")
        || p.contains("/login/")
        || p.contains("/auth/")
}

/// Lower-cased host and the path (with query) of an `http(s)` URL, without a url
/// crate. `None` for anything that is not an absolute http(s) URL.
fn split_host_path(url: &str) -> Option<(String, String)> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    // Drop any userinfo and port.
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    let host = authority.split(':').next().unwrap_or(authority);
    Some((host.to_ascii_lowercase(), path.to_string()))
}

/// Runs the whole flow: launch, connect, capture, store nothing (the caller
/// decides what to do with the session). Returns the captured session and how.
pub async fn capture(opts: &LoginOptions) -> Result<(Session, Captured)> {
    let bin = match &opts.browser {
        Some(b) => b.clone(),
        None => find_browser().ok_or_else(|| {
            IdxError::Usage(
                "no Chromium-family browser found; set IDX_BROWSER=/path/to/chrome, or use \
                 `idx stockbit login --manual` to paste the token"
                    .into(),
            )
        })?,
    };
    std::fs::create_dir_all(&opts.profile_dir)?;

    let port = pick_port();
    let mut child = spawn_browser(&bin, &opts.profile_dir, port)?;
    let guard = ChildGuard(&mut child);

    let result = tokio::time::timeout(opts.timeout, capture_loop(port, opts.timeout)).await;
    drop(guard); // kills the browser whatever happened
    match result {
        Ok(r) => r,
        Err(_) => Err(IdxError::provider(
            super::NAME,
            "login timed out before a session was captured",
        )),
    }
}

/// Connects to the browser's DevTools endpoint and watches for the token.
async fn capture_loop(port: u16, timeout: Duration) -> Result<(Session, Captured)> {
    let ws_url = discover_ws(port, Duration::from_secs(30)).await?;
    let cdp = Cdp::connect(&ws_url).await?;

    // Arm new targets, then open the login page as one.
    cdp.fire(
        "Target.setAutoAttach",
        json!({"autoAttach": true, "waitForDebuggerOnStart": true, "flatten": true}),
        None,
    )
    .await?;
    cdp.fire("Target.createTarget", json!({"url": LOGIN_URL}), None)
        .await?;

    let deadline = tokio::time::Instant::now() + timeout;
    let mut cookie_tick = tokio::time::interval(Duration::from_secs(3));
    cookie_tick.tick().await; // the first tick is immediate; skip it

    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => {
                return Err(IdxError::provider(super::NAME, "login timed out"));
            }
            _ = cookie_tick.tick() => {
                if let Some(session) = read_cookie_session(&cdp).await {
                    return Ok((session, Captured::AlreadySignedIn));
                }
            }
            event = cdp.next_event() => {
                let Some((method, params, sid)) = event else {
                    return Err(IdxError::provider(super::NAME, "the browser closed the DevTools connection"));
                };
                if let Some(session) = handle_event(&cdp, &method, &params, sid.as_deref()).await? {
                    return Ok((session, Captured::Login));
                }
            }
        }
    }
}

/// Reacts to one CDP event; returns a session when a token is captured.
async fn handle_event(
    cdp: &Cdp,
    method: &str,
    params: &Value,
    sid: Option<&str>,
) -> Result<Option<Session>> {
    match method {
        // A new target (the login page, a popup) — arm it and let it run.
        "Target.attachedToTarget" => {
            if let Some(s) = params.get("sessionId").and_then(|s| s.as_str()) {
                cdp.fire("Network.enable", json!({}), Some(s)).await.ok();
                cdp.fire(
                    "Fetch.enable",
                    json!({"patterns": [
                        {"urlPattern": "*stockbit.com/login/*", "requestStage": "Response"},
                        {"urlPattern": "*stockbit.com/auth/*", "requestStage": "Response"},
                    ]}),
                    Some(s),
                )
                .await
                .ok();
                cdp.fire("Runtime.runIfWaitingForDebugger", json!({}), Some(s))
                    .await
                    .ok();
            }
            Ok(None)
        }
        // A paused response: read its body if it is a token URL, then release it.
        "Fetch.requestPaused" => {
            let request_id = params
                .get("requestId")
                .and_then(|r| r.as_str())
                .unwrap_or("");
            let url = params
                .get("request")
                .and_then(|r| r.get("url"))
                .and_then(|u| u.as_str())
                .unwrap_or("");
            let has_response = params.get("responseStatusCode").is_some();
            let mut found = None;
            if has_response && token_url_allowed(url) {
                if let Ok(body) = cdp
                    .send(
                        "Fetch.getResponseBody",
                        json!({"requestId": request_id}),
                        sid,
                        Duration::from_secs(5),
                    )
                    .await
                {
                    found = session_from_response_body(&body);
                }
            }
            // Always release the request, or the page hangs on a spinner.
            let _ = cdp
                .send(
                    "Fetch.continueRequest",
                    json!({"requestId": request_id}),
                    sid,
                    Duration::from_secs(5),
                )
                .await;
            Ok(found)
        }
        _ => Ok(None),
    }
}

/// Extracts a session from a `Fetch.getResponseBody` result (`{body, base64Encoded}`).
fn session_from_response_body(result: &Value) -> Option<Session> {
    let raw = result.get("body").and_then(|b| b.as_str())?;
    let text = if result.get("base64Encoded").and_then(|b| b.as_bool()) == Some(true) {
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, raw).ok()?;
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        raw.to_string()
    };
    let s = super::auth::parse_refresh(&text, &Session::default()).ok()?;
    (!s.refresh.trim().is_empty()).then_some(s)
}

/// Reads the `credentialStorage` cookie for an already-signed-in browser.
async fn read_cookie_session(cdp: &Cdp) -> Option<Session> {
    let result = cdp
        .send(
            "Storage.getCookies",
            json!({}),
            None,
            Duration::from_secs(5),
        )
        .await
        .ok()?;
    let cookies = result.get("cookies")?.as_array()?;
    for c in cookies {
        let name = c.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let domain = c.get("domain").and_then(|d| d.as_str()).unwrap_or("");
        if name == "credentialStorage" && domain.contains("stockbit.com") {
            if let Some(value) = c.get("value").and_then(|v| v.as_str()) {
                if let Some(s) = session_from_credential_blob(value) {
                    if !s.refresh.trim().is_empty() {
                        return Some(s);
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Browser process
// ---------------------------------------------------------------------------

fn pick_port() -> u16 {
    // A random high port, like the reference launcher; the exact value does not
    // matter as long as it is free, and discover_ws confirms it opened.
    9500 + (std::process::id() % 400) as u16
}

fn spawn_browser(bin: &std::path::Path, profile: &std::path::Path, port: u16) -> Result<Child> {
    Command::new(bin)
        .arg(format!("--remote-debugging-port={port}"))
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--new-window")
        .arg("about:blank")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| IdxError::provider(super::NAME, format!("could not launch the browser: {e}")))
}

/// Kills the browser when the login finishes, however it finishes.
struct ChildGuard<'a>(&'a mut Child);
impl Drop for ChildGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The browser-level WebSocket URL, once the debugging port answers.
async fn discover_ws(port: u16, timeout: Duration) -> Result<String> {
    let url = format!("http://127.0.0.1:{port}/json/version");
    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(resp) = client.get(&url).send().await {
            if let Ok(text) = resp.text().await {
                if let Ok(v) = serde_json::from_str::<Value>(&text) {
                    if let Some(ws) = v.get("webSocketDebuggerUrl").and_then(|w| w.as_str()) {
                        return Ok(ws.to_string());
                    }
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(IdxError::provider(
                super::NAME,
                "the browser's debugging port never opened; close any window using this profile and retry",
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// Common install locations for Chromium-family browsers, plus `IDX_BROWSER` /
/// `STOCKBIT_BROWSER` overrides. Firefox and Safari do not speak CDP and are
/// not listed.
pub fn find_browser() -> Option<PathBuf> {
    for key in ["IDX_BROWSER", "STOCKBIT_BROWSER"] {
        if let Some(p) = std::env::var_os(key) {
            let p = PathBuf::from(p);
            if p.exists() {
                return Some(p);
            }
        }
    }
    let candidates: &[&str] = if cfg!(windows) {
        &[
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
            r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe",
        ]
    } else if cfg!(target_os = "macos") {
        &[
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
        ]
    } else {
        &[
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/microsoft-edge",
            "/usr/bin/brave-browser",
        ]
    };
    candidates.iter().map(PathBuf::from).find(|p| p.exists())
}

// ---------------------------------------------------------------------------
// Minimal CDP client over a localhost WebSocket (no TLS)
// ---------------------------------------------------------------------------

use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{client_async, WebSocketStream};

type Pending = Arc<Mutex<std::collections::HashMap<u64, oneshot::Sender<Value>>>>;
type Writer = Arc<Mutex<futures::stream::SplitSink<WebSocketStream<TcpStream>, Message>>>;

/// A tiny Chrome DevTools Protocol client: correlates command results by id and
/// streams events to the caller. A background task owns the read half.
struct Cdp {
    next_id: AtomicU64,
    writer: Writer,
    pending: Pending,
    events: Mutex<tokio::sync::mpsc::UnboundedReceiver<(String, Value, Option<String>)>>,
}

impl Cdp {
    async fn connect(ws_url: &str) -> Result<Cdp> {
        let (host, port, _) = parse_ws_url(ws_url)?;
        let tcp = TcpStream::connect((host.as_str(), port))
            .await
            .map_err(|e| {
                IdxError::provider(super::NAME, format!("DevTools connect failed: {e}"))
            })?;
        let (ws, _) = client_async(ws_url, tcp).await.map_err(|e| {
            IdxError::provider(super::NAME, format!("DevTools handshake failed: {e}"))
        })?;
        let (sink, mut stream) = ws.split();
        let pending: Pending = Arc::new(Mutex::new(std::collections::HashMap::new()));
        let (ev_tx, ev_rx) = tokio::sync::mpsc::unbounded_channel();
        let reader_pending = pending.clone();
        tokio::spawn(async move {
            while let Some(Ok(msg)) = stream.next().await {
                let Ok(text) = msg.into_text() else { continue };
                let Ok(v) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                if let Some(id) = v.get("id").and_then(|i| i.as_u64()) {
                    if let Some(tx) = reader_pending.lock().await.remove(&id) {
                        let payload = v.get("result").cloned().unwrap_or_else(|| json!({}));
                        let _ = tx.send(payload);
                    }
                } else if let Some(method) = v.get("method").and_then(|m| m.as_str()) {
                    let params = v.get("params").cloned().unwrap_or_else(|| json!({}));
                    let sid = v
                        .get("sessionId")
                        .and_then(|s| s.as_str())
                        .map(String::from);
                    let _ = ev_tx.send((method.to_string(), params, sid));
                }
            }
        });
        Ok(Cdp {
            next_id: AtomicU64::new(1),
            writer: Arc::new(Mutex::new(sink)),
            pending,
            events: Mutex::new(ev_rx),
        })
    }

    async fn write(
        &self,
        id: u64,
        method: &str,
        params: Value,
        session: Option<&str>,
    ) -> Result<()> {
        let mut msg = json!({"id": id, "method": method, "params": params});
        if let Some(s) = session {
            msg["sessionId"] = json!(s);
        }
        self.writer
            .lock()
            .await
            .send(Message::text(msg.to_string()))
            .await
            .map_err(|e| IdxError::provider(super::NAME, format!("DevTools write failed: {e}")))
    }

    /// Sends a command without waiting for its result.
    async fn fire(&self, method: &str, params: Value, session: Option<&str>) -> Result<()> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.write(id, method, params, session).await
    }

    /// Sends a command and waits for its result, bounded by `timeout`.
    async fn send(
        &self,
        method: &str,
        params: Value,
        session: Option<&str>,
        timeout: Duration,
    ) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);
        self.write(id, method, params, session).await?;
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(v)) => Ok(v),
            _ => {
                self.pending.lock().await.remove(&id);
                Err(IdxError::provider(
                    super::NAME,
                    format!("{method} timed out"),
                ))
            }
        }
    }

    async fn next_event(&self) -> Option<(String, Value, Option<String>)> {
        self.events.lock().await.recv().await
    }
}

/// Splits `ws://host:port/path` into host, port, path. CDP endpoints are always
/// plain `ws://` on loopback.
fn parse_ws_url(url: &str) -> Result<(String, u16, String)> {
    let rest = url
        .strip_prefix("ws://")
        .ok_or_else(|| IdxError::provider(super::NAME, "DevTools URL was not ws://"))?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = authority.split_once(':').unwrap_or((authority, "80"));
    let port = port
        .parse()
        .map_err(|_| IdxError::provider(super::NAME, "DevTools URL had no port"))?;
    Ok((host.to_string(), port, format!("/{path}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_url_rules() {
        assert!(token_url_allowed(
            "https://exodus.stockbit.com/login/v3/username/browser"
        ));
        assert!(token_url_allowed(
            "https://exodus.stockbit.com/auth/v2/login"
        ));
        assert!(!token_url_allowed(
            "https://api-sekuritas.stockbit.com/partner/eipo/access_token"
        ));
        assert!(!token_url_allowed(
            "https://exodus.stockbit.com/login/v3/otp"
        ));
        assert!(!token_url_allowed(
            "https://carina.stockbit.com/auth/v2/login"
        ));
        assert!(!token_url_allowed(
            "https://evil.test/exodus.stockbit.com/login/"
        ));
    }

    #[test]
    fn parses_ws_urls() {
        let (h, p, path) = parse_ws_url("ws://127.0.0.1:9541/devtools/browser/abc").unwrap();
        assert_eq!(h, "127.0.0.1");
        assert_eq!(p, 9541);
        assert_eq!(path, "/devtools/browser/abc");
        assert!(parse_ws_url("http://x").is_err());
    }

    #[test]
    fn reads_a_base64_login_body() {
        let body = include_str!("../../../tests/fixtures/stockbit/login_refresh_constructed.json");
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, body);
        let result = json!({"body": b64, "base64Encoded": true});
        let s = session_from_response_body(&result).unwrap();
        assert!(!s.refresh.is_empty());
    }
}
