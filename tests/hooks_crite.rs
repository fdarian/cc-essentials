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
    let out = Command::cargo_bin("cc-essentials")
        .unwrap()
        .args(["hooks", "crite"])
        .env("CC_ESSENTIALS_LOG", "0") // explicitly disable log noise
        .write_stdin(stdin)
        .output()
        .unwrap();
    (
        String::from_utf8(out.stdout).unwrap(),
        out.status.code().unwrap_or(-1),
    )
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
