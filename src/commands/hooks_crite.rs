use crate::biome::{run::BiomeOutcome, summary};
use crate::cache::Cache;
use crate::detect;
use crate::error_dump::{self, LastErrorPayload};
use crate::hook_io::{HookInput, HookOutput, HookSpecificOutput};
use crate::log;
use anyhow::Result;
use serde_json::json;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Entrypoint. Reads hook stdin, attempts to format + lint, writes hook JSON to stdout.
/// NEVER returns Err — all errors are coerced to an empty (but valid) JSON hook output.
pub fn run(cache: &Cache, stdin: &mut dyn Read, stdout: &mut dyn Write) -> Result<()> {
    let output = run_inner(cache, stdin).unwrap_or_else(|_| HookOutput::default());
    let payload = serde_json::to_string(&output).unwrap_or_else(|_| "{}".to_string());
    let _ = writeln!(stdout, "{payload}");
    Ok(())
}

fn run_inner(cache: &Cache, stdin: &mut dyn Read) -> Result<HookOutput> {
    let mut buf = String::new();
    stdin.read_to_string(&mut buf)?;

    let input: HookInput = match serde_json::from_str(&buf) {
        Ok(i) => i,
        Err(_) => {
            log::log_event(cache.dir(), "hook.stdin_parse_failed", json!({}));
            return Ok(HookOutput::default());
        }
    };

    // 1. Only Write / Edit / MultiEdit touch files.
    if !matches!(input.tool_name.as_str(), "Write" | "Edit" | "MultiEdit") {
        log::log_event(
            cache.dir(),
            "hook.skip_unsupported_tool",
            json!({ "tool": input.tool_name }),
        );
        return Ok(HookOutput::default());
    }

    // 2. Must have a file_path.
    let Some(file_path_str) = input.tool_input.file_path.as_ref() else {
        log::log_event(
            cache.dir(),
            "hook.skip_missing_file_path",
            json!({ "tool": input.tool_name }),
        );
        return Ok(HookOutput::default());
    };
    let file_path = PathBuf::from(file_path_str);

    // 3. Must be a supported extension.
    if !is_supported_extension(&file_path) {
        log::log_event(
            cache.dir(),
            "hook.skip_unsupported_extension",
            json!({ "path": file_path_str }),
        );
        return Ok(HookOutput::default());
    }

    // 4. File must exist (it might have been deleted if this is a weird edge case).
    if !file_path.exists() {
        log::log_event(
            cache.dir(),
            "hook.skip_missing_file",
            json!({ "path": file_path_str }),
        );
        return Ok(HookOutput::default());
    }

    // 5. Detect biome config + binary from the file's parent directory.
    let parent = file_path.parent().unwrap_or(Path::new("."));
    let detected = detect::detect_from(parent, cache)?;
    let Some(biome) = detected.biome else {
        log::log_event(
            cache.dir(),
            "hook.skip_no_biome",
            json!({ "path": file_path_str }),
        );
        return Ok(HookOutput::default());
    };

    // 6. Compute the file path relative to the biome config's directory (biome uses cwd=config_dir).
    let config_dir = match biome.config_path.parent() {
        Some(p) => p.to_path_buf(),
        None => {
            log::log_event(
                cache.dir(),
                "hook.no_config_parent",
                json!({ "config": biome.config_path.display().to_string() }),
            );
            return Ok(HookOutput::default());
        }
    };
    let config_path = biome.config_path.clone();
    let binary_path = biome.binary_path.clone();
    let file_canonical = std::fs::canonicalize(&file_path).unwrap_or(file_path.clone());
    let config_dir_canonical = std::fs::canonicalize(&config_dir).unwrap_or(config_dir.clone());
    let relative = match file_canonical.strip_prefix(&config_dir_canonical) {
        Ok(r) => r.to_path_buf(),
        Err(_) => {
            // If we can't make it relative (shouldn't happen since biome.json was walked up from here),
            // fall back to passing the absolute path — biome accepts either.
            file_canonical.clone()
        }
    };

    // 7. Run biome.
    let argv: Vec<String> = vec![
        "check".to_string(),
        "--write".to_string(),
        "--reporter=json".to_string(),
        relative.to_string_lossy().into_owned(),
    ];
    let outcome = crate::biome::run::run_check(&binary_path, &config_dir_canonical, &relative);

    let display = file_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_path_str.clone());
    let sys_msg = summary::system_message(&display, &outcome);
    let additional = match &outcome {
        BiomeOutcome::Parsed { check, .. } => summary::additional_context(check, &display),
        _ => None,
    };

    // 8. Write last-error.json (always-on) and emit enriched log events for non-Parsed outcomes.
    match &outcome {
        BiomeOutcome::Parsed { .. } => {
            log::log_event(
                cache.dir(),
                "hook.completed",
                json!({
                    "path": file_path_str,
                    "outcome": outcome_kind(&outcome),
                    "has_additional_context": additional.is_some(),
                }),
            );
        }
        BiomeOutcome::FallbackText {
            stdout,
            stderr,
            exit_code,
        } => {
            let payload = LastErrorPayload {
                ts_unix_ms: error_dump::current_ts_unix_ms(),
                event: "fallback_text",
                tool_name: input.tool_name.clone(),
                file_path: file_path_str.clone(),
                biome_binary: Some(binary_path.clone()),
                biome_config: Some(config_path.clone()),
                argv: argv.clone(),
                cwd: Some(config_dir_canonical.clone()),
                exit_code: *exit_code,
                stdout_first_4k: error_dump::truncate_utf8(stdout, 4096),
                stderr_first_4k: error_dump::truncate_utf8(stderr, 4096),
                error: None,
            };
            error_dump::write_last_error(cache.dir(), &payload);
            log::log_event(
                cache.dir(),
                "hook.completed",
                json!({
                    "path": file_path_str,
                    "outcome": outcome_kind(&outcome),
                    "has_additional_context": additional.is_some(),
                    "exit_code": exit_code,
                    "stdout_first_1k": error_dump::truncate_utf8(stdout, 1024),
                    "stderr_first_1k": error_dump::truncate_utf8(stderr, 1024),
                }),
            );
        }
        BiomeOutcome::SpawnFailed { error } => {
            let payload = LastErrorPayload {
                ts_unix_ms: error_dump::current_ts_unix_ms(),
                event: "spawn_failed",
                tool_name: input.tool_name.clone(),
                file_path: file_path_str.clone(),
                biome_binary: Some(binary_path.clone()),
                biome_config: Some(config_path.clone()),
                argv: argv.clone(),
                cwd: Some(config_dir_canonical.clone()),
                exit_code: None,
                stdout_first_4k: String::new(),
                stderr_first_4k: String::new(),
                error: Some(error.clone()),
            };
            error_dump::write_last_error(cache.dir(), &payload);
            log::log_event(
                cache.dir(),
                "hook.completed",
                json!({
                    "path": file_path_str,
                    "outcome": outcome_kind(&outcome),
                    "has_additional_context": additional.is_some(),
                    "exit_code": serde_json::Value::Null,
                    "error": error,
                }),
            );
        }
    }

    Ok(HookOutput {
        system_message: Some(sys_msg),
        hook_specific_output: Some(HookSpecificOutput {
            hook_event_name: "PostToolUse",
            additional_context: additional,
        }),
    })
}

fn outcome_kind(o: &BiomeOutcome) -> &'static str {
    match o {
        BiomeOutcome::Parsed { .. } => "parsed",
        BiomeOutcome::FallbackText { .. } => "fallback_text",
        BiomeOutcome::SpawnFailed { .. } => "spawn_failed",
    }
}

fn is_supported_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts" | "json" | "jsonc"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn extension_detection() {
        for ext in [
            "ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "json", "jsonc",
        ] {
            assert!(is_supported_extension(Path::new(&format!("foo.{ext}"))));
        }
        for ext in ["md", "rs", "py", "go", "html", "css"] {
            assert!(!is_supported_extension(Path::new(&format!("foo.{ext}"))));
        }
        assert!(!is_supported_extension(Path::new("no_extension")));
        // Uppercase accepted
        assert!(is_supported_extension(Path::new("FOO.TS")));
    }
}
