//! The CLI's loading indicator. While a command waits on the network, one
//! line on stderr names the hosts it is waiting on and for how long, so a
//! source that stalls (a news feed that accepts the connection and never
//! answers, IPOT's spurious 404s being retried) shows up as itself instead of
//! a silent terminal.
//!
//! The fetcher reports each live request through [`request`]; nothing is
//! recorded unless the CLI turned the indicator on with [`start`], which it
//! does only when stderr is a terminal ([`wanted`]). Scripts, agents and
//! `idx mcp` capture stderr, so they never see it. `IDX_PROGRESS=never`
//! turns it off; `IDX_PROGRESS=always` shows it on a stderr that does not
//! report itself as a terminal (mintty, some IDE consoles). Anything else
//! written to stderr goes through [`suspend`], which takes the line down
//! first.

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// A command that answers within this (a cache hit) never shows the line.
const DELAY: Duration = Duration::from_millis(300);
const TICK: Duration = Duration::from_millis(100);
/// Hosts named on the line; the rest are counted.
const SHOWN: usize = 2;

const FRAMES: &[&str] = if cfg!(windows) {
    // Legacy console fonts have no braille.
    &["-", "\\", "|", "/"]
} else {
    &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static NEXT_ID: AtomicU64 = AtomicU64::new(0);
static STATE: Mutex<State> = Mutex::new(State::new());
static WAKE: Condvar = Condvar::new();

struct State {
    running: bool,
    started: Option<Instant>,
    /// (id, label, since) of every request still waiting.
    inflight: Vec<(u64, String, Instant)>,
    done: u64,
    /// Characters of the line on screen; 0 when none is drawn.
    drawn: usize,
}

impl State {
    const fn new() -> Self {
        State {
            running: false,
            started: None,
            inflight: Vec::new(),
            done: 0,
            drawn: 0,
        }
    }
}

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether the CLI should show the indicator: no `-v` logs share the line,
/// and stderr is a terminal (not `TERM=dumb`) unless `IDX_PROGRESS` says
/// `always` or `never` (also `0`, `off`, `false`, `no`).
pub fn wanted(
    stderr_is_terminal: bool,
    verbosity: u8,
    env: &dyn Fn(&str) -> Option<String>,
) -> bool {
    if verbosity > 0 {
        return false;
    }
    let setting = env("IDX_PROGRESS").map(|v| v.trim().to_ascii_lowercase());
    match setting.as_deref() {
        Some("always" | "1" | "on" | "true" | "yes") => true,
        Some("never" | "0" | "off" | "false" | "no") => false,
        _ => stderr_is_terminal && env("TERM").as_deref() != Some("dumb"),
    }
}

/// The running indicator; dropping it takes the line down.
pub struct Spinner {
    thread: Option<JoinHandle<()>>,
}

/// Turns the indicator on until the returned guard drops. `None` when one is
/// already running.
pub fn start() -> Option<Spinner> {
    if ACTIVE.swap(true, Ordering::SeqCst) {
        return None;
    }
    {
        let mut st = state();
        *st = State::new();
        st.running = true;
        st.started = Some(Instant::now());
    }
    let thread = std::thread::Builder::new()
        .name("idx-progress".into())
        .spawn(ticker)
        .ok();
    Some(Spinner { thread })
}

impl Drop for Spinner {
    fn drop(&mut self) {
        finish();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Stops the indicator for good and clears its line. Called before the
/// command's output, so nothing is drawn after it.
pub fn finish() {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    let mut st = state();
    st.running = false;
    st.inflight.clear();
    clear(&mut st);
    WAKE.notify_all();
}

/// Runs `f` with the line taken down, so whatever it writes to stderr lands
/// on a clean line; the indicator redraws on its next tick.
pub fn suspend<R>(f: impl FnOnce() -> R) -> R {
    if !ACTIVE.load(Ordering::SeqCst) {
        return f();
    }
    let mut st = state();
    clear(&mut st);
    f()
}

/// One live request, named by its URL's host until the guard drops.
pub fn request(url: &str) -> Pending {
    track(host_of(url).to_string(), true)
}

/// A pause before asking `url` again, shown as `host retry n`.
pub fn retrying(url: &str, attempt: usize) -> Pending {
    track(format!("{} retry {attempt}", host_of(url)), false)
}

fn track(label: String, counts: bool) -> Pending {
    if !ACTIVE.load(Ordering::SeqCst) {
        return Pending(None);
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    state().inflight.push((id, label, Instant::now()));
    Pending(Some((id, counts)))
}

/// Guard for one entry on the line.
pub struct Pending(Option<(u64, bool)>);

impl Drop for Pending {
    fn drop(&mut self) {
        if let Some((id, counts)) = self.0 {
            let mut st = state();
            let before = st.inflight.len();
            st.inflight.retain(|(i, ..)| *i != id);
            if counts && st.inflight.len() < before {
                st.done += 1;
            }
        }
    }
}

fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    &rest[..end]
}

fn ticker() {
    let mut frame = 0usize;
    let mut st = state();
    loop {
        if !st.running {
            return;
        }
        let since = st.started.map_or(Duration::ZERO, |s| s.elapsed());
        let wait = if since < DELAY {
            DELAY - since
        } else {
            draw(&mut st, frame);
            frame = frame.wrapping_add(1);
            TICK
        };
        st = WAKE
            .wait_timeout(st, wait)
            .map(|(g, _)| g)
            .unwrap_or_else(|e| e.into_inner().0);
    }
}

fn draw(st: &mut State, frame: usize) {
    let now = Instant::now();
    let waits: Vec<(&str, Duration)> = st
        .inflight
        .iter()
        .map(|(_, label, since)| (label.as_str(), now.duration_since(*since)))
        .collect();
    let elapsed = st.started.map_or(Duration::ZERO, |s| now.duration_since(s));
    let width =
        terminal_size::terminal_size_of(std::io::stderr()).map_or(80, |(w, _)| w.0 as usize);
    let line = render(
        FRAMES[frame % FRAMES.len()],
        elapsed,
        &waits,
        st.done,
        width,
    );
    let len = line.chars().count();
    let pad = st.drawn.saturating_sub(len);
    let mut err = std::io::stderr().lock();
    let _ = write!(err, "\r{line}{:pad$}", "");
    let _ = err.flush();
    st.drawn = len;
}

fn clear(st: &mut State) {
    if st.drawn == 0 {
        return;
    }
    let mut err = std::io::stderr().lock();
    let _ = write!(err, "\r{:w$}\r", "", w = st.drawn);
    let _ = err.flush();
    st.drawn = 0;
}

/// The line: frame, total time, what it is waiting on (longest wait first,
/// one entry per host), how many requests already answered. Cut to fit
/// `width` so it never wraps.
fn render(
    frame: &str,
    elapsed: Duration,
    waits: &[(&str, Duration)],
    done: u64,
    width: usize,
) -> String {
    let mut hosts: Vec<(&str, usize, Duration)> = Vec::new();
    for (label, age) in waits {
        match hosts.iter_mut().find(|(l, ..)| l == label) {
            Some(h) => {
                h.1 += 1;
                h.2 = h.2.max(*age);
            }
            None => hosts.push((label, 1, *age)),
        }
    }
    hosts.sort_by_key(|h| std::cmp::Reverse(h.2));

    let mut line = format!("{frame} {}", secs(elapsed));
    if hosts.is_empty() {
        line.push_str(" · working");
    } else {
        let named: Vec<String> = hosts
            .iter()
            .take(SHOWN)
            .map(|(label, n, age)| {
                let times = if *n > 1 {
                    format!(" ×{n}")
                } else {
                    String::new()
                };
                format!("{label}{times} {}", secs(*age))
            })
            .collect();
        line.push_str(" · waiting on ");
        line.push_str(&named.join(", "));
        if hosts.len() > SHOWN {
            line.push_str(&format!(" +{} more", hosts.len() - SHOWN));
        }
    }
    if done > 0 {
        line.push_str(&format!(" · {done} answered"));
    }
    // One column short of the edge: a full row wraps on some terminals.
    let max = width.saturating_sub(1).max(10);
    if line.chars().count() > max {
        line = line.chars().take(max - 1).collect::<String>() + "…";
    }
    line
}

fn secs(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else {
        format!("{}m{:02}s", s / 60, s % 60)
    }
}

/// `tracing` writer that takes the line down before each log line.
pub struct Stderr;

impl Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        suspend(|| std::io::stderr().write(buf))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::stderr().flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        }
    }

    #[test]
    fn shown_only_on_a_quiet_terminal() {
        assert!(wanted(true, 0, &env(&[])));
        assert!(!wanted(false, 0, &env(&[])));
        assert!(!wanted(true, 1, &env(&[])));
        assert!(!wanted(true, 0, &env(&[("IDX_PROGRESS", "never")])));
        assert!(!wanted(true, 0, &env(&[("IDX_PROGRESS", "0")])));
        assert!(wanted(true, 0, &env(&[("IDX_PROGRESS", "auto")])));
        assert!(!wanted(true, 0, &env(&[("TERM", "dumb")])));
        assert!(wanted(false, 0, &env(&[("IDX_PROGRESS", "always")])));
        assert!(!wanted(false, 2, &env(&[("IDX_PROGRESS", "always")])));
    }

    #[test]
    fn hosts_from_urls() {
        assert_eq!(
            host_of("https://www.kontan.co.id/rss/investasi"),
            "www.kontan.co.id"
        );
        assert_eq!(
            host_of("https://query1.finance.yahoo.com?x=1"),
            "query1.finance.yahoo.com"
        );
        assert_eq!(host_of("www.idx.co.id"), "www.idx.co.id");
    }

    #[test]
    fn names_the_longest_wait_first() {
        let s = |n| Duration::from_secs(n);
        let line = render(
            "-",
            s(7),
            &[("www.cnbcindonesia.com", s(1)), ("www.kontan.co.id", s(6))],
            3,
            120,
        );
        assert_eq!(
            line,
            "- 7s · waiting on www.kontan.co.id 6s, www.cnbcindonesia.com 1s · 3 answered"
        );
    }

    #[test]
    fn groups_a_host_and_counts_the_rest() {
        let s = |n| Duration::from_secs(n);
        let waits = [
            ("query2.finance.yahoo.com", s(2)),
            ("query2.finance.yahoo.com", s(3)),
            ("assets.msn.com", s(1)),
            ("www.indopremier.com retry 2", s(0)),
        ];
        let line = render("-", s(75), &waits, 0, 200);
        assert_eq!(
            line,
            "- 1m15s · waiting on query2.finance.yahoo.com ×2 3s, assets.msn.com 1s +1 more"
        );
        assert_eq!(render("-", s(1), &[], 0, 80), "- 1s · working");
    }

    #[test]
    fn never_wider_than_the_terminal() {
        let line = render(
            "-",
            Duration::ZERO,
            &[("a-very-long-host-name.example.com", Duration::ZERO)],
            0,
            30,
        );
        assert_eq!(line.chars().count(), 29);
        assert!(line.ends_with('…'));
    }

    #[test]
    fn requests_are_ignored_while_off() {
        // Not started: the guard records nothing.
        let p = request("https://example.com/x");
        assert!(p.0.is_none());
    }
}
