use assert_cmd::Command;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;

#[cfg(unix)]
fn make_exec(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    fs::write(path, body).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn run_hook(stdin: &str, home: &Path) -> (String, i32) {
    let output = Command::cargo_bin("cc-essentials")
        .unwrap()
        .args(["codex", "hooks", "crite"])
        .env("CC_ESSENTIALS_LOG", "0")
        .env("HOME", home)
        .env_remove("XDG_CACHE_HOME")
        .write_stdin(stdin)
        .output()
        .unwrap();
    (
        String::from_utf8(output.stdout).unwrap(),
        output.status.code().unwrap_or(-1),
    )
}

fn parse_json(output: &str) -> Value {
    serde_json::from_str(output.trim()).unwrap_or_else(|error| panic!("{error}: {output}"))
}

#[test]
fn command_accepts_empty_stdin_and_exits_zero() {
    let home = tempfile::tempdir().unwrap();
    let (output, code) = run_hook("", home.path());
    assert_eq!(code, 0);
    assert_eq!(parse_json(&output), json!({}));
}

#[test]
fn command_accepts_garbage_stdin_and_exits_zero() {
    let home = tempfile::tempdir().unwrap();
    let (output, code) = run_hook("not json", home.path());
    assert_eq!(code, 0);
    assert_eq!(parse_json(&output), json!({}));
}

#[test]
#[cfg(unix)]
fn formats_relative_apply_patch_target_and_returns_codex_output() {
    let project = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    fs::write(project.path().join("biome.json"), "{}").unwrap();
    fs::write(project.path().join("foo.ts"), "const x = 1;\n").unwrap();

    let binary_dir = project.path().join("node_modules/.bin");
    fs::create_dir_all(&binary_dir).unwrap();
    let fixture = include_str!("fixtures/biome_warnings.json");
    make_exec(
        &binary_dir.join("biome"),
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Version: 1.9.4'; exit 0; fi\ncat <<'EOF'\n{fixture}\nEOF\n"
        ),
    );

    let input = json!({
        "session_id": "session-1",
        "cwd": project.path(),
        "hook_event_name": "PostToolUse",
        "tool_name": "apply_patch",
        "tool_use_id": "tool-1",
        "tool_input": {
            "command": "*** Begin Patch\n*** Update File: foo.ts\n@@\n-const x = 1;\n+const x = 2;\n*** End Patch\n"
        },
        "tool_response": { "success": true },
        "model": "gpt-5",
        "permission_mode": "default",
        "transcript_path": null,
        "turn_id": "turn-1"
    });

    let (output, code) = run_hook(&input.to_string(), home.path());
    assert_eq!(code, 0);
    let value = parse_json(&output);
    assert_eq!(
        value["systemMessage"].as_str().unwrap(),
        "cc-essentials: formatted foo.ts (2 warnings)"
    );
    assert_eq!(value["hookSpecificOutput"]["hookEventName"], "PostToolUse");
    assert!(value["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap()
        .contains("biome report for foo.ts"));
}

#[test]
#[cfg(unix)]
fn formats_all_files_from_a_single_raw_patch_input() {
    let project = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    fs::write(project.path().join("biome.json"), "{}").unwrap();
    fs::write(project.path().join("one.ts"), "const one = 1;\n").unwrap();
    fs::write(project.path().join("two.ts"), "const two = 2;\n").unwrap();

    let binary_dir = project.path().join("node_modules/.bin");
    fs::create_dir_all(&binary_dir).unwrap();
    let marker = project.path().join("biome-runs.txt");
    let fixture = include_str!("fixtures/biome_empty.json");
    make_exec(
        &binary_dir.join("biome"),
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Version: 1.9.4'; exit 0; fi\necho \"$4\" >> {}\ncat <<'EOF'\n{fixture}\nEOF\n",
            marker.display()
        ),
    );

    let raw_patch = "*** Begin Patch\n*** Update File: one.ts\n@@\n*** Update File: two.ts\n@@\n*** End Patch\n";
    let input = json!({
        "cwd": project.path(),
        "hook_event_name": "PostToolUse",
        "tool_name": "apply_patch",
        "tool_input": raw_patch,
        "tool_response": null
    });

    let (output, code) = run_hook(&input.to_string(), home.path());
    assert_eq!(code, 0);
    let value = parse_json(&output);
    let system_message = value["systemMessage"].as_str().unwrap();
    assert!(system_message.contains("formatted one.ts"));
    assert!(system_message.contains("formatted two.ts"));
    let runs = fs::read_to_string(marker).unwrap();
    assert_eq!(runs.lines().count(), 2);
}

#[test]
#[cfg(unix)]
fn bash_apply_patch_is_supported_for_codex_versions_that_wrap_the_patch() {
    let project = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    fs::write(project.path().join("biome.json"), "{}").unwrap();
    fs::write(project.path().join("foo.ts"), "const x = 1;\n").unwrap();

    let binary_dir = project.path().join("node_modules/.bin");
    fs::create_dir_all(&binary_dir).unwrap();
    let fixture = include_str!("fixtures/biome_empty.json");
    make_exec(
        &binary_dir.join("biome"),
        &format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'Version: 1.9.4'; exit 0; fi\ncat <<'EOF'\n{fixture}\nEOF\n"
        ),
    );

    let input = json!({
        "cwd": project.path(),
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": {
            "command": "apply_patch <<'PATCH'\n*** Begin Patch\n*** Update File: foo.ts\n*** End Patch\nPATCH"
        },
        "tool_response": { "success": true }
    });

    let (output, code) = run_hook(&input.to_string(), home.path());
    assert_eq!(code, 0);
    assert_eq!(
        parse_json(&output)["systemMessage"],
        "cc-essentials: formatted foo.ts"
    );
}

#[test]
#[cfg(unix)]
fn wrong_event_and_unsupported_tool_are_silent() {
    let home = tempfile::tempdir().unwrap();
    let event = json!({
        "cwd": "/tmp",
        "hook_event_name": "PreToolUse",
        "tool_name": "apply_patch",
        "tool_input": "*** Begin Patch\n*** Update File: foo.ts\n*** End Patch"
    });
    let (event_output, event_code) = run_hook(&event.to_string(), home.path());
    assert_eq!(event_code, 0);
    assert_eq!(parse_json(&event_output), json!({}));

    let tool = json!({
        "cwd": "/tmp",
        "hook_event_name": "PostToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": "printf hello" }
    });
    let (tool_output, tool_code) = run_hook(&tool.to_string(), home.path());
    assert_eq!(tool_code, 0);
    assert_eq!(parse_json(&tool_output), json!({}));
}
