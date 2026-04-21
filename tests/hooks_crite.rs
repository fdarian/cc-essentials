use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

#[cfg(unix)]
fn make_exec(p: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(p, body).unwrap();
    let mut perms = fs::metadata(p).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(p, perms).unwrap();
}

fn run_hook(stdin: &str) -> (String, i32) {
    // Isolate every invocation under a throwaway HOME so tests never
    // touch the developer's real ~/Library/Caches or ~/.cache. The
    // TempDir is dropped at the end of this function — we don't need
    // to inspect the cache afterwards.
    let tmp = tempfile::tempdir().unwrap();
    let out = Command::cargo_bin("cc-essentials")
        .unwrap()
        .args(["hooks", "crite"])
        .env("CC_ESSENTIALS_LOG", "0") // explicitly disable log noise
        .env("HOME", tmp.path())
        .env_remove("XDG_CACHE_HOME")
        .write_stdin(stdin)
        .output()
        .unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        out.status.code().unwrap_or(-1),
    )
}

#[cfg(unix)]
fn run_hook_with_home(stdin: &str, home: &Path) -> (String, i32) {
    let out = Command::cargo_bin("cc-essentials")
        .unwrap()
        .args(["hooks", "crite"])
        .env("CC_ESSENTIALS_LOG", "0")
        .env("HOME", home)
        .env_remove("XDG_CACHE_HOME")
        .write_stdin(stdin)
        .output()
        .unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        out.status.code().unwrap_or(-1),
    )
}

#[cfg(unix)]
fn run_hook_with_log(stdin: &str, home: &Path) -> (String, i32) {
    let out = Command::cargo_bin("cc-essentials")
        .unwrap()
        .args(["hooks", "crite"])
        .env("CC_ESSENTIALS_LOG", "1")
        .env("HOME", home)
        .env_remove("XDG_CACHE_HOME")
        .write_stdin(stdin)
        .output()
        .unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        out.status.code().unwrap_or(-1),
    )
}

#[cfg(unix)]
fn expected_cache_dir(home: &Path) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        home.join("Library/Caches/cc-essentials/detect")
    }
    #[cfg(not(target_os = "macos"))]
    {
        home.join(".cache/cc-essentials/detect")
    }
}

fn parse_json_or_empty(s: &str) -> Value {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return json!({});
    }
    serde_json::from_str(trimmed).unwrap_or(json!({}))
}

#[test]
fn exit_0_on_empty_stdin() {
    let (_, code) = run_hook("");
    assert_eq!(code, 0);
}

#[test]
fn exit_0_on_garbage_stdin() {
    let (_, code) = run_hook("not json {{{");
    assert_eq!(code, 0);
}

#[test]
fn exit_0_on_unknown_tool() {
    let (stdout, code) = run_hook(r#"{"tool_name":"Bash","tool_input":{"command":"ls"}}"#);
    assert_eq!(code, 0);
    let v = parse_json_or_empty(&stdout);
    assert!(v.get("systemMessage").is_none());
}

#[test]
fn exit_0_on_unsupported_extension() {
    let (stdout, code) =
        run_hook(r#"{"tool_name":"Write","tool_input":{"file_path":"/tmp/foo.md"}}"#);
    assert_eq!(code, 0);
    let v = parse_json_or_empty(&stdout);
    assert!(v.get("systemMessage").is_none());
}

#[test]
fn exit_0_on_missing_file_path() {
    let (stdout, code) = run_hook(r#"{"tool_name":"Write","tool_input":{}}"#);
    assert_eq!(code, 0);
    let v = parse_json_or_empty(&stdout);
    assert!(v.get("systemMessage").is_none());
}

#[test]
fn exit_0_when_file_does_not_exist() {
    let (stdout, code) = run_hook(
        r#"{"tool_name":"Write","tool_input":{"file_path":"/tmp/does_not_exist_xyz_123.ts"}}"#,
    );
    assert_eq!(code, 0);
    let _ = parse_json_or_empty(&stdout);
}

#[test]
#[cfg(unix)]
fn emits_system_message_and_additional_context_on_findings() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    // minimal biome project
    fs::write(root.join("biome.json"), "{}").unwrap();
    fs::write(root.join("pnpm-lock.yaml"), "").unwrap();
    fs::write(root.join("foo.ts"), "const x = 1;\n").unwrap();
    let bin_dir = root.join("node_modules/.bin");
    fs::create_dir_all(&bin_dir).unwrap();
    // stub biome: respond to --version, otherwise emit the warnings fixture
    let fixture = include_str!("fixtures/biome_warnings.json");
    make_exec(
        &bin_dir.join("biome"),
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Version: 1.9.4'; exit 0; fi\ncat <<'EOF'\n{fixture}\nEOF\nexit 0\n"
        ),
    );

    let abs_foo = root.join("foo.ts");
    let input = format!(
        r#"{{"tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        abs_foo.display()
    );
    let (stdout, code) = run_hook(&input);
    assert_eq!(code, 0);
    let v = parse_json_or_empty(&stdout);
    let sys = v
        .get("systemMessage")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    assert!(sys.starts_with("cc-essentials:"), "systemMessage: {sys}");
    let hook_out = v.get("hookSpecificOutput").unwrap();
    assert_eq!(hook_out.get("hookEventName").unwrap(), "PostToolUse");
    let ac = hook_out
        .get("additionalContext")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    assert!(ac.contains("warning"), "additionalContext: {ac}");
}

#[test]
#[cfg(unix)]
fn emits_empty_additional_context_on_clean_run() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    fs::write(root.join("biome.json"), "{}").unwrap();
    fs::write(root.join("foo.ts"), "const x = 1;\n").unwrap();
    let bin_dir = root.join("node_modules/.bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let fixture = include_str!("fixtures/biome_empty.json");
    let fixture_one = fixture.replace(r#""changed": 0"#, r#""changed": 1"#);
    make_exec(
        &bin_dir.join("biome"),
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Version: 1.9.4'; exit 0; fi\ncat <<'EOF'\n{fixture_one}\nEOF\nexit 0\n"
        ),
    );

    let abs_foo = root.join("foo.ts");
    let input = format!(
        r#"{{"tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        abs_foo.display()
    );
    let (stdout, code) = run_hook(&input);
    assert_eq!(code, 0);
    let v = parse_json_or_empty(&stdout);
    let sys = v
        .get("systemMessage")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    assert_eq!(sys, "cc-essentials: formatted foo.ts");
    let hook_out = v.get("hookSpecificOutput").unwrap();
    // additionalContext is None when no diagnostics
    assert!(hook_out.get("additionalContext").is_none());
}

#[test]
#[cfg(unix)]
fn exit_0_when_biome_config_missing_from_project() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    fs::write(root.join("foo.ts"), "").unwrap();
    // no biome.json anywhere in this tempdir chain
    let input = format!(
        r#"{{"tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        root.join("foo.ts").display()
    );
    let (stdout, code) = run_hook(&input);
    assert_eq!(code, 0);
    // stdout should be valid JSON (possibly {}), not a panic trace
    let _ = parse_json_or_empty(&stdout);
}

#[test]
#[cfg(unix)]
fn exit_0_on_biome_fallback_text() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    fs::write(root.join("biome.json"), "{}").unwrap();
    fs::write(root.join("foo.ts"), "").unwrap();
    let bin_dir = root.join("node_modules/.bin");
    fs::create_dir_all(&bin_dir).unwrap();
    make_exec(
        &bin_dir.join("biome"),
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Version: 1.9.4'; exit 0; fi\necho 'not valid json' 1>&2\nexit 1\n",
    );

    let abs_foo = root.join("foo.ts");
    let input = format!(
        r#"{{"tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        abs_foo.display()
    );
    let (stdout, code) = run_hook(&input);
    assert_eq!(code, 0);
    let v = parse_json_or_empty(&stdout);
    let sys = v
        .get("systemMessage")
        .and_then(|s| s.as_str())
        .unwrap_or("");
    assert!(sys.contains("biome error"), "systemMessage: {sys}");
}

#[test]
#[cfg(unix)]
fn last_error_dump_written_on_fallback_text() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    let home = t.path().join("fake_home");
    fs::create_dir_all(&home).unwrap();

    fs::write(root.join("biome.json"), "{}").unwrap();
    // Use pnpm-lock.yaml so detection finds a lockfile (not required for fallback but realistic)
    fs::write(root.join("foo.ts"), "").unwrap();
    let bin_dir = root.join("node_modules/.bin");
    fs::create_dir_all(&bin_dir).unwrap();
    // Stub: --version works, everything else emits garbage JSON to stdout and exits 0
    make_exec(
        &bin_dir.join("biome"),
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Version: 1.9.4'; exit 0; fi\necho 'this is not valid json garbage'\nexit 0\n",
    );

    let abs_foo = root.join("foo.ts");
    let input = format!(
        r#"{{"tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        abs_foo.display()
    );
    let (_, code) = run_hook_with_home(&input, &home);
    assert_eq!(code, 0);

    let cache_dir = expected_cache_dir(&home);
    let last_error_path = cache_dir.join("last-error.json");
    assert!(
        last_error_path.exists(),
        "last-error.json should exist at {last_error_path:?}"
    );

    let contents = fs::read_to_string(&last_error_path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&contents).unwrap();

    assert_eq!(v["event"], "fallback_text");
    assert_eq!(v["tool_name"], "Write");
    assert!(
        v["file_path"].as_str().unwrap().ends_with("foo.ts"),
        "file_path: {}",
        v["file_path"]
    );
    assert!(
        v["stdout_first_4k"]
            .as_str()
            .unwrap()
            .contains("this is not valid json garbage"),
        "stdout_first_4k: {}",
        v["stdout_first_4k"]
    );
    let argv: Vec<&str> = v["argv"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert!(argv.contains(&"check"));
    assert!(argv.contains(&"--write"));
    assert!(argv.contains(&"--reporter=json"));
    assert_eq!(v["exit_code"], 0);
}

#[test]
#[cfg(unix)]
fn log_event_includes_stdout_on_fallback_text() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path();
    let home = t.path().join("fake_home");
    fs::create_dir_all(&home).unwrap();

    fs::write(root.join("biome.json"), "{}").unwrap();
    fs::write(root.join("foo.ts"), "").unwrap();
    let bin_dir = root.join("node_modules/.bin");
    fs::create_dir_all(&bin_dir).unwrap();
    make_exec(
        &bin_dir.join("biome"),
        "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Version: 1.9.4'; exit 0; fi\necho 'garbage_for_log_test'\nexit 0\n",
    );

    let abs_foo = root.join("foo.ts");
    let input = format!(
        r#"{{"tool_name":"Write","tool_input":{{"file_path":"{}"}}}}"#,
        abs_foo.display()
    );
    let (_, code) = run_hook_with_log(&input, &home);
    assert_eq!(code, 0);

    let cache_dir = expected_cache_dir(&home);
    let log_path = cache_dir.join("hooks.log");
    assert!(log_path.exists(), "hooks.log should exist at {log_path:?}");

    let log_contents = fs::read_to_string(&log_path).unwrap();
    // Find the hook.completed line for fallback_text
    let last_line = log_contents
        .lines()
        .rfind(|l| l.contains("hook.completed") && l.contains("fallback_text"))
        .expect("should find a hook.completed fallback_text event");

    let v: serde_json::Value = serde_json::from_str(last_line).unwrap();
    assert!(
        v["stdout_first_1k"]
            .as_str()
            .unwrap()
            .contains("garbage_for_log_test"),
        "stdout_first_1k: {}",
        v["stdout_first_1k"]
    );
    assert_eq!(v["exit_code"], 0);
}
