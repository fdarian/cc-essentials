use super::run::BiomeOutcome;
use super::schema::{CheckOutput, Diagnostic, Location, Severity};
use std::fmt::Write;

const MAX_DIAGNOSTICS_IN_CONTEXT: usize = 50;

/// Produce the LLM-facing summary. Returns `None` when there is nothing to report.
pub fn additional_context(check: &CheckOutput, file_display: &str) -> Option<String> {
    if check.diagnostics.is_empty() {
        return None;
    }
    let mut diags: Vec<&Diagnostic> = check.diagnostics.iter().collect();
    diags.sort_by_key(|d| severity_rank(d.severity));

    let mut s = String::new();
    let _ = writeln!(
        s,
        "biome report for {file_display}: {} error(s), {} warning(s)",
        check.summary.errors, check.summary.warnings
    );
    let total = diags.len();
    for d in diags.iter().take(MAX_DIAGNOSTICS_IN_CONTEXT) {
        let loc = format_loc(d);
        let cat = d.category.as_deref().unwrap_or("-");
        let sev = severity_label(d.severity);
        let _ = writeln!(s, "  {loc} {sev}({cat}): {}", d.message);
    }
    if total > MAX_DIAGNOSTICS_IN_CONTEXT {
        let more = total - MAX_DIAGNOSTICS_IN_CONTEXT;
        let _ = writeln!(s, "  ... +{more} more diagnostic(s) omitted");
    }
    Some(s)
}

/// Produce the user-visible terminal summary.
pub fn system_message(file_display: &str, outcome: &BiomeOutcome) -> String {
    match outcome {
        BiomeOutcome::Parsed { check, .. } => {
            let wrote = check.summary.changed > 0;
            let errs = check.summary.errors;
            let warns = check.summary.warnings;
            if errs > 0 && !wrote {
                format!(
                    "cc-essentials: skipped {file_display} ({errs} error{} — biome left the file unchanged)",
                    if errs == 1 { "" } else { "s" }
                )
            } else if errs > 0 {
                format!(
                    "cc-essentials: formatted {file_display} with {errs} error{} remaining",
                    plural(errs)
                )
            } else if warns > 0 {
                format!(
                    "cc-essentials: formatted {file_display} ({warns} warning{})",
                    plural(warns)
                )
            } else {
                format!("cc-essentials: formatted {file_display}")
            }
        }
        BiomeOutcome::FallbackText {
            exit_code, stderr, ..
        } => {
            // Non-JSON biome output. Two common cases:
            // - exit 0: biome ran fine but emitted a reporter the parser didn't understand.
            //   Skip stderr noise like the `--json is unstable` warning.
            // - non-zero: genuine biome error; surface the first stderr line.
            let note = stderr
                .lines()
                .find(|l| {
                    let t = l.trim();
                    !t.is_empty()
                        && !t.contains("--json option is unstable")
                        && !t.contains("--json is unstable")
                })
                .unwrap_or("")
                .trim();
            match exit_code {
                Some(0) => format!(
                    "cc-essentials: ran biome on {file_display} but couldn't parse its output{}",
                    if note.is_empty() { String::new() } else { format!(" ({note})") }
                ),
                _ => format!(
                    "cc-essentials: biome error running against {file_display} (exit {exit_code:?}): {note}"
                ),
            }
        }
        BiomeOutcome::SpawnFailed { error } => {
            format!("cc-essentials: failed to run biome against {file_display}: {error}")
        }
    }
}

fn plural(n: u64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

fn severity_rank(s: Severity) -> u8 {
    match s {
        Severity::Fatal => 0,
        Severity::Error => 1,
        Severity::Warning => 2,
        Severity::Hint => 3,
        Severity::Info => 4,
    }
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Fatal => "fatal",
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Hint => "hint",
        Severity::Info => "info",
    }
}

fn format_loc(d: &Diagnostic) -> String {
    let path = d
        .location
        .as_ref()
        .and_then(|l| l.path.as_ref())
        .map(|p| p.display())
        .unwrap_or_else(|| "?".to_string());
    let (line, col) = d.location.as_ref().and_then(line_col).unwrap_or((0, 0));
    format!("{path}:{line}:{col}")
}

/// Resolve line/column from a biome diagnostic location.
///
/// Prefers the legacy `start: {line, column}` shape (what our older
/// fixtures emit). Falls back to biome 2.x's byte-offset `span` + the
/// attached `sourceCode`, walking the source to convert offset to
/// (line, column). Returns `None` when neither is usable.
fn line_col(loc: &Location) -> Option<(u64, u64)> {
    if let Some(start) = loc.start {
        if let (Some(line), Some(col)) = (start.line, start.column) {
            return Some((line, col));
        }
    }
    let (start_offset, _) = loc.span?;
    let source = loc.source_code.as_deref()?;
    let mut line: u64 = 1;
    let mut col: u64 = 1;
    for (i, c) in source.char_indices() {
        if i as u64 >= start_offset {
            return Some((line, col));
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    Some((line, col))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(fixture: &str) -> CheckOutput {
        serde_json::from_str(fixture).unwrap()
    }

    #[test]
    fn additional_context_none_on_clean() {
        let c = parse(include_str!("../../tests/fixtures/biome_empty.json"));
        assert!(additional_context(&c, "foo.ts").is_none());
    }

    #[test]
    fn additional_context_lists_warnings() {
        let c = parse(include_str!("../../tests/fixtures/biome_warnings.json"));
        let s = additional_context(&c, "index.ts").unwrap();
        assert!(s.contains("2 warning(s)"));
        assert!(s.contains("lint/correctness/noUnusedImports"));
        assert!(s.contains("lint/suspicious/noExplicitAny"));
    }

    #[test]
    fn additional_context_orders_errors_before_warnings() {
        let c = parse(include_str!("../../tests/fixtures/biome_errors.json"));
        let s = additional_context(&c, "src/foo.ts").unwrap();
        let err_idx = s.find("error(").unwrap();
        let warn_idx = s.find("warning(").unwrap();
        assert!(
            err_idx < warn_idx,
            "errors should appear before warnings:\n{s}"
        );
    }

    #[test]
    fn system_message_clean_run() {
        let c = parse(include_str!("../../tests/fixtures/biome_empty.json"));
        // fixture has changed=0 (nothing to format). Force changed=1 to simulate a formatted write.
        let mut c = c;
        c.summary.changed = 1;
        let outcome = BiomeOutcome::Parsed {
            check: c,
            relative_file: "foo.ts".into(),
            exit_code: Some(0),
        };
        assert_eq!(
            system_message("foo.ts", &outcome),
            "cc-essentials: formatted foo.ts"
        );
    }

    #[test]
    fn system_message_with_warnings() {
        let mut c = parse(include_str!("../../tests/fixtures/biome_warnings.json"));
        c.summary.changed = 1;
        let outcome = BiomeOutcome::Parsed {
            check: c,
            relative_file: "index.ts".into(),
            exit_code: Some(0),
        };
        assert_eq!(
            system_message("index.ts", &outcome),
            "cc-essentials: formatted index.ts (2 warnings)"
        );
    }

    #[test]
    fn system_message_skipped_on_parse_error_with_no_write() {
        let c = parse(include_str!("../../tests/fixtures/biome_parse_error.json"));
        // fixture has changed=0 and errors=1 — simulate biome's default: don't write on parse errors
        let outcome = BiomeOutcome::Parsed {
            check: c,
            relative_file: "src/foo.ts".into(),
            exit_code: Some(1),
        };
        assert!(system_message("src/foo.ts", &outcome)
            .starts_with("cc-essentials: skipped src/foo.ts (1 error"));
    }

    #[test]
    fn system_message_spawn_failed() {
        let outcome = BiomeOutcome::SpawnFailed {
            error: "no such file".to_string(),
        };
        let s = system_message("foo.ts", &outcome);
        assert!(s.contains("failed to run biome"));
        assert!(s.contains("foo.ts"));
    }

    #[test]
    fn system_message_fallback_text() {
        let outcome = BiomeOutcome::FallbackText {
            stdout: "".into(),
            stderr: "biome: fatal: config is invalid\n(more details)".into(),
            exit_code: Some(2),
        };
        let s = system_message("foo.ts", &outcome);
        assert!(s.contains("biome error"));
        assert!(s.contains("foo.ts"));
        assert!(s.contains("fatal: config is invalid"));
    }

    #[test]
    fn diagnostic_cap_applied() {
        let mut c = parse(include_str!("../../tests/fixtures/biome_warnings.json"));
        // inflate diagnostics to > cap
        while c.diagnostics.len() < MAX_DIAGNOSTICS_IN_CONTEXT + 5 {
            c.diagnostics.push(c.diagnostics[0].clone());
        }
        let s = additional_context(&c, "index.ts").unwrap();
        assert!(s.contains("+5 more"));
    }
}
