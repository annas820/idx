//! End-to-end tests of the binary that never touch the network.
//! Live-provider checks live in `scripts/smoke.sh`.

use assert_cmd::Command;
use predicates::prelude::*;

fn idx(cache: &std::path::Path, config: &std::path::Path) -> Command {
    let mut cmd = Command::cargo_bin("idx").unwrap();
    cmd.env_clear()
        .env("IDX_CACHE_DIR", cache)
        .env("IDX_CONFIG", config)
        .env("NO_COLOR", "1")
        // Safety net: these tests must never reach the network, whatever a
        // test does. A cache miss then fails with exit 5 instead of fetching.
        .env("IDX_OFFLINE", "1")
        // Keep the test machine's own settings out of the picture.
        .env_remove("IDX_PROVIDER")
        .env_remove("IDX_OUTPUT");
    // SystemRoot is needed on Windows for the TLS/crypto providers to initialise.
    if let Ok(v) = std::env::var("SystemRoot") {
        cmd.env("SystemRoot", v);
    }
    cmd
}

struct Sandbox {
    _dir: tempfile::TempDir,
    cache: std::path::PathBuf,
    config: std::path::PathBuf,
}

fn sandbox() -> Sandbox {
    let dir = tempfile::tempdir().unwrap();
    Sandbox {
        cache: dir.path().join("cache"),
        config: dir.path().join("config.toml"),
        _dir: dir,
    }
}

#[test]
fn help_lists_commands_and_exit_codes() {
    let sb = sandbox();
    idx(&sb.cache, &sb.config)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("stocks"))
        .stdout(predicate::str::contains("Exit codes"));
    idx(&sb.cache, &sb.config)
        .args(["stocks", "quote", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("idx stocks quote BBCA,BBRI,TLKM"));
}

#[test]
fn version_in_both_formats() {
    let sb = sandbox();
    idx(&sb.cache, &sb.config)
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(format!(
            "idx {}",
            env!("CARGO_PKG_VERSION")
        )));
    let out = idx(&sb.cache, &sb.config)
        .args(["-o", "json", "version"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
}

#[test]
fn schema_outputs_are_valid_json_schema() {
    let sb = sandbox();
    for name in ["quotes", "history", "error", "quote", "candle"] {
        let out = idx(&sb.cache, &sb.config)
            .args(["schema", name])
            .output()
            .unwrap();
        assert!(out.status.success(), "{name}");
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(
            v["$schema"].as_str().unwrap().contains("json-schema.org"),
            "{name}"
        );
    }
    idx(&sb.cache, &sb.config)
        .args(["schema", "nope"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unknown schema"));
}

#[test]
fn usage_errors_exit_2() {
    let sb = sandbox();
    idx(&sb.cache, &sb.config)
        .args(["stocks", "quote"])
        .assert()
        .code(2);
    idx(&sb.cache, &sb.config)
        .args(["stocks", "quote", "BB$CA"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid symbol"));
    idx(&sb.cache, &sb.config)
        .args(["stocks", "history", "BBCA", "--period", "7d"])
        .assert()
        .code(2);
    idx(&sb.cache, &sb.config)
        .args(["--offline", "--no-cache", "version"])
        .assert()
        .code(2);
}

#[test]
fn offline_with_empty_cache_exits_5_and_json_error_is_structured() {
    let sb = sandbox();
    idx(&sb.cache, &sb.config)
        .args(["--offline", "stocks", "quote", "BBCA"])
        .assert()
        .code(5)
        .stderr(predicate::str::contains("offline"))
        .stderr(predicate::str::contains("hint:"));

    let out = idx(&sb.cache, &sb.config)
        .args(["--offline", "-o", "json", "stocks", "history", "BBCA"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(out.stdout.is_empty(), "errors never go to stdout");
    let v: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(v["error"]["code"], "offline_miss");
    assert_eq!(v["exit_code"], 5);
}

#[test]
fn config_init_set_get_roundtrip() {
    let sb = sandbox();
    idx(&sb.cache, &sb.config)
        .args(["config", "path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("config.toml"));
    idx(&sb.cache, &sb.config)
        .args(["config", "init"])
        .assert()
        .success();
    assert!(sb.config.exists());
    idx(&sb.cache, &sb.config)
        .args(["config", "init"])
        .assert()
        .code(6)
        .stderr(predicate::str::contains("already exists"));
    idx(&sb.cache, &sb.config)
        .args(["config", "set", "provider", "yahoo"])
        .assert()
        .success();
    idx(&sb.cache, &sb.config)
        .args(["config", "set", "provider", "bloomberg"])
        .assert()
        .code(6);
    idx(&sb.cache, &sb.config)
        .args(["config", "get", "provider"])
        .assert()
        .success()
        .stdout("yahoo\n");
    idx(&sb.cache, &sb.config)
        .args(["config", "get"])
        .assert()
        .success()
        .stdout(predicate::str::contains("provider    : yahoo"));

    // Env beats file, CLI beats env.
    let out = idx(&sb.cache, &sb.config)
        .env("IDX_PROVIDER", "auto")
        .args(["-o", "json", "config", "get"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["provider"], "auto");
    let out = idx(&sb.cache, &sb.config)
        .env("IDX_PROVIDER", "auto")
        .args(["--provider", "yahoo", "-o", "json", "config", "get"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["provider"], "yahoo");
    assert_eq!(v["cache_dir"], sb.cache.display().to_string());
}

#[test]
fn msn_provider_is_not_available_yet() {
    let sb = sandbox();
    idx(&sb.cache, &sb.config)
        .args(["--provider", "msn", "stocks", "quote", "BBCA"])
        .assert()
        .code(6)
        .stderr(predicate::str::contains("msn"));
}

#[test]
fn cache_commands() {
    let sb = sandbox();
    idx(&sb.cache, &sb.config)
        .args(["cache", "path"])
        .assert()
        .success()
        .stdout(format!("{}\n", sb.cache.display()));
    idx(&sb.cache, &sb.config)
        .args(["cache", "stats"])
        .assert()
        .success()
        .stdout(predicate::str::contains("files : 0"));
    let out = idx(&sb.cache, &sb.config)
        .args(["-o", "json", "cache", "clear"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["removed"], 0);
}

#[test]
fn completions_generate() {
    let sb = sandbox();
    idx(&sb.cache, &sb.config)
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_idx"));
    idx(&sb.cache, &sb.config)
        .args(["completions", "powershell"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Register-ArgumentCompleter"));
}
