use assert_cmd::Command;
use std::fs;
use std::path::Path;

fn run_logs_cmd(home: &Path) -> String {
    let out = Command::cargo_bin("cc-essentials")
        .unwrap()
        .args(["logs"])
        .env("CC_ESSENTIALS_LOG", "0")
        .env("HOME", home)
        .env_remove("XDG_CACHE_HOME")
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

#[cfg(target_os = "macos")]
fn cache_dir(home: &Path) -> std::path::PathBuf {
    home.join("Library/Caches/cc-essentials/detect")
}

#[cfg(not(target_os = "macos"))]
fn cache_dir(home: &Path) -> std::path::PathBuf {
    home.join(".cache/cc-essentials/detect")
}

#[test]
fn logs_command_prints_paths_when_nothing_exists() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().join("home");
    fs::create_dir_all(&home).unwrap();

    let output = run_logs_cmd(&home);

    assert!(
        output.contains("log enabled: no"),
        "expected 'log enabled: no', got: {output}"
    );
    assert!(
        output.contains("hooks.log"),
        "expected log file path, got: {output}"
    );
    assert!(
        output.contains("last-error.json"),
        "expected last error path, got: {output}"
    );
    assert!(
        output.contains("no last error recorded"),
        "expected 'no last error recorded', got: {output}"
    );
    assert!(
        output.contains("no log entries"),
        "expected 'no log entries', got: {output}"
    );
}

#[test]
fn logs_command_shows_last_error_when_present() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().join("home");
    let cache = cache_dir(&home);
    fs::create_dir_all(&cache).unwrap();

    let fake_payload = serde_json::json!({
        "ts_unix_ms": 1713123456789_u64,
        "event": "fallback_text",
        "tool_name": "Write",
        "file_path": "/some/path/foo.ts",
        "exit_code": 0,
        "stdout_first_4k": "this is the garbage output",
        "stderr_first_4k": ""
    });
    fs::write(
        cache.join("last-error.json"),
        serde_json::to_string_pretty(&fake_payload).unwrap(),
    )
    .unwrap();

    let output = run_logs_cmd(&home);

    assert!(
        output.contains("--- last error ---"),
        "expected last error header, got: {output}"
    );
    assert!(
        output.contains("fallback_text"),
        "expected event name, got: {output}"
    );
    assert!(
        output.contains("this is the garbage output"),
        "expected stdout contents, got: {output}"
    );
}
