use crate::cache::Cache;
use crate::commands::hooks_crite::{self, FileHookResult};
use crate::hook_io::{HookOutput, HookSpecificOutput};
use crate::log;
use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct CodexHookInput {
    #[serde(default)]
    hook_event_name: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    tool_name: String,
    #[serde(default)]
    tool_input: Value,
}

/// Entrypoint for Codex PostToolUse hooks.
///
/// Codex invokes command hooks with one JSON event on stdin and accepts the
/// same `systemMessage`/`hookSpecificOutput` fields used by Claude Code. The
/// hook is informational, so every failure becomes an empty successful result.
pub fn run(cache: &Cache, stdin: &mut dyn Read, stdout: &mut dyn Write) -> Result<()> {
    let output = run_inner(cache, stdin).unwrap_or_else(|_| HookOutput::default());
    let payload = serde_json::to_string(&output).unwrap_or_else(|_| "{}".to_string());
    let _ = writeln!(stdout, "{payload}");
    Ok(())
}

fn run_inner(cache: &Cache, stdin: &mut dyn Read) -> Result<HookOutput> {
    let mut buf = String::new();
    stdin.read_to_string(&mut buf)?;

    let input: CodexHookInput = match serde_json::from_str(&buf) {
        Ok(input) => input,
        Err(_) => {
            log::log_event(cache.dir(), "codex_hook.stdin_parse_failed", json!({}));
            return Ok(HookOutput::default());
        }
    };

    if let Some(event_name) = input.hook_event_name.as_deref() {
        if event_name != "PostToolUse" {
            log::log_event(
                cache.dir(),
                "codex_hook.skip_wrong_event",
                json!({ "event": event_name }),
            );
            return Ok(HookOutput::default());
        }
    }

    let raw_paths = match input.tool_name.as_str() {
        "apply_patch" => patch_paths_from_value(&input.tool_input),
        "Bash" => bash_paths_from_value(&input.tool_input),
        "Write" | "Edit" | "MultiEdit" => file_paths_from_value(&input.tool_input),
        _ => {
            log::log_event(
                cache.dir(),
                "codex_hook.skip_unsupported_tool",
                json!({ "tool": input.tool_name }),
            );
            return Ok(HookOutput::default());
        }
    };

    let cwd = input.cwd.as_deref().map(Path::new);
    let mut paths = Vec::new();
    for raw_path in raw_paths {
        let Some(path) = resolve_path(&raw_path, cwd) else {
            log::log_event(
                cache.dir(),
                "codex_hook.skip_relative_path_without_cwd",
                json!({ "path": raw_path }),
            );
            continue;
        };
        if !paths.iter().any(|existing: &PathBuf| existing == &path) {
            paths.push(path);
        }
    }

    if paths.is_empty() {
        log::log_event(
            cache.dir(),
            "codex_hook.skip_no_file_paths",
            json!({ "tool": input.tool_name }),
        );
        return Ok(HookOutput::default());
    }

    let mut results = Vec::new();
    for path in paths {
        match hooks_crite::process_file(cache, &input.tool_name, &path) {
            Ok(Some(result)) => results.push(result),
            Ok(None) => {}
            Err(error) => {
                log::log_event(
                    cache.dir(),
                    "codex_hook.file_failed",
                    json!({
                        "tool": input.tool_name,
                        "path": path.display().to_string(),
                        "error": error.to_string(),
                    }),
                );
            }
        }
    }

    if results.is_empty() {
        return Ok(HookOutput::default());
    }

    Ok(output_for_results(results))
}

fn output_for_results(results: Vec<FileHookResult>) -> HookOutput {
    let mut system_messages = Vec::with_capacity(results.len());
    let mut additional_contexts = Vec::new();
    for result in results {
        system_messages.push(result.system_message);
        if let Some(context) = result.additional_context {
            additional_contexts.push(context);
        }
    }

    HookOutput {
        system_message: Some(system_messages.join("\n")),
        hook_specific_output: Some(HookSpecificOutput {
            hook_event_name: "PostToolUse",
            additional_context: if additional_contexts.is_empty() {
                None
            } else {
                Some(additional_contexts.join("\n\n"))
            },
        }),
    }
}

fn patch_paths_from_value(value: &Value) -> Vec<String> {
    let Some(raw) = text_from_value(value, &["command", "input", "patch"]) else {
        return Vec::new();
    };
    parse_patch_paths(&raw)
}

fn bash_paths_from_value(value: &Value) -> Vec<String> {
    let Some(raw) = text_from_value(value, &["command"]) else {
        return Vec::new();
    };

    let patch_paths = parse_patch_paths(&raw);
    if !patch_paths.is_empty() {
        return patch_paths;
    }
    parse_redirect_paths(&raw)
}

fn file_paths_from_value(value: &Value) -> Vec<String> {
    let Some(path) = text_from_value(value, &["file_path", "filePath", "path"]) else {
        return Vec::new();
    };
    let patch_paths = parse_patch_paths(&path);
    if !patch_paths.is_empty() {
        return patch_paths;
    }
    if path.trim().is_empty() {
        Vec::new()
    } else {
        vec![path]
    }
}

fn text_from_value(value: &Value, fields: &[&str]) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Object(object) => {
            for field in fields {
                if let Some(Value::String(text)) = object.get(*field) {
                    return Some(text.clone());
                }
            }
            None
        }
        _ => None,
    }
}

fn parse_patch_paths(raw: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut current_index = None;

    for line in raw.lines() {
        let line = line.trim();
        if line == "*** End Patch" {
            break;
        }

        let operation = ["*** Add File:", "*** Update File:", "*** Delete File:"]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix));
        if let Some(path) = operation.map(str::trim).filter(|path| !path.is_empty()) {
            paths.push(path.to_string());
            current_index = Some(paths.len() - 1);
            continue;
        }

        if let Some(path) = line
            .strip_prefix("*** Move to:")
            .map(str::trim)
            .filter(|path| !path.is_empty())
        {
            if let Some(index) = current_index {
                paths[index] = path.to_string();
            } else {
                paths.push(path.to_string());
                current_index = Some(paths.len() - 1);
            }
        }
    }

    paths
}

fn parse_redirect_paths(raw: &str) -> Vec<String> {
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    let mut paths = Vec::new();

    for index in 0..tokens.len() {
        let token = tokens[index];
        if token == "tee" {
            if let Some(path) = tokens.get(index + 1).and_then(|token| clean_token(token)) {
                if path != "-" {
                    paths.push(path.to_string());
                }
            }
            continue;
        }

        if token == ">" {
            if let Some(path) = tokens.get(index + 1).and_then(|token| clean_token(token)) {
                paths.push(path.to_string());
            }
        } else if let Some(path) = token.strip_prefix('>') {
            if !path.starts_with('>') {
                if let Some(path) = clean_token(path) {
                    paths.push(path.to_string());
                }
            }
        }
    }

    paths
}

fn clean_token(token: &str) -> Option<&str> {
    let token = token.trim_matches(|character| character == '\'' || character == '"');
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn resolve_path(raw_path: &str, cwd: Option<&Path>) -> Option<PathBuf> {
    let path = Path::new(raw_path.trim());
    if path.as_os_str().is_empty() {
        return None;
    }
    if path.is_absolute() {
        Some(path.to_path_buf())
    } else {
        cwd.map(|directory| directory.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_multiple_patch_operations() {
        let raw = "*** Begin Patch\n*** Update File: src/one.ts\n@@\n*** Add File: src/two.ts\n+new\n*** End Patch\n";
        assert_eq!(
            parse_patch_paths(raw),
            vec!["src/one.ts".to_string(), "src/two.ts".to_string()]
        );
    }

    #[test]
    fn move_replaces_source_path() {
        let raw = "*** Begin Patch\n*** Update File: src/one.ts\n*** Move to: src/two.ts\n*** End Patch\n";
        assert_eq!(parse_patch_paths(raw), vec!["src/two.ts".to_string()]);
    }

    #[test]
    fn accepts_string_and_object_codex_inputs() {
        let raw = "*** Begin Patch\n*** Update File: foo.ts\n*** End Patch";
        assert_eq!(
            patch_paths_from_value(&Value::String(raw.to_string())),
            vec!["foo.ts".to_string()]
        );
        assert_eq!(
            patch_paths_from_value(&json!({ "command": raw })),
            vec!["foo.ts".to_string()]
        );
    }

    #[test]
    fn parses_bash_patch_and_redirect() {
        let patch =
            "apply_patch <<'PATCH'\n*** Begin Patch\n*** Update File: foo.ts\n*** End Patch\nPATCH";
        assert_eq!(
            bash_paths_from_value(&json!({ "command": patch })),
            vec!["foo.ts".to_string()]
        );
        assert_eq!(
            bash_paths_from_value(&json!({ "command": "cat <<'EOF' > foo.ts" })),
            vec!["foo.ts".to_string()]
        );
    }
}
