use serde::{Deserialize, Deserializer};
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutput {
    #[serde(default)]
    pub summary: Summary,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    #[serde(default)]
    pub command: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    #[serde(default)]
    pub changed: u64,
    #[serde(default)]
    pub unchanged: u64,
    #[serde(default)]
    pub matches: u64,
    #[serde(default)]
    pub errors: u64,
    #[serde(default)]
    pub warnings: u64,
    #[serde(default)]
    pub infos: u64,
    #[serde(default)]
    pub skipped: u64,
    #[serde(default)]
    pub suggested_fixes_skipped: u64,
    #[serde(default)]
    pub diagnostics_not_printed: u64,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Plaintext message. Biome 2.x emits `description` as a flat string
    /// alongside a styled `message` array; we prefer the former, falling
    /// back to concatenating `content` segments from the latter so older
    /// biome versions (and our legacy fixtures) keep working.
    pub message: String,
    pub category: Option<String>,
    pub location: Option<Location>,
}

impl<'de> Deserialize<'de> for Diagnostic {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            severity: Severity,
            #[serde(default)]
            description: Option<String>,
            #[serde(default)]
            message: Option<serde_json::Value>,
            #[serde(default)]
            category: Option<String>,
            #[serde(default)]
            location: Option<Location>,
        }
        let r = Raw::deserialize(d)?;
        let message = r
            .description
            .or_else(|| r.message.and_then(flatten_message))
            .unwrap_or_default();
        Ok(Diagnostic {
            severity: r.severity,
            message,
            category: r.category,
            location: r.location,
        })
    }
}

/// Accepts biome's `message` field in either form:
/// - biome 1.x / our legacy fixtures: `"message": "text"`
/// - biome 2.x: `"message": [{elements: [...], content: "..."}, ...]`
fn flatten_message(v: serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(s),
        serde_json::Value::Array(segments) => {
            let mut out = String::new();
            for seg in segments {
                if let Some(c) = seg.get("content").and_then(|c| c.as_str()) {
                    out.push_str(c);
                }
            }
            if out.is_empty() {
                None
            } else {
                Some(out)
            }
        }
        _ => None,
    }
}

#[derive(Debug, Default, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    #[default]
    Info,
    Warning,
    Error,
    Hint,
    Fatal,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Location {
    #[serde(default)]
    pub path: Option<PathOrObject>,
    /// Biome 2.x: byte offsets `[start, end]` into the source.
    #[serde(default)]
    pub span: Option<(u64, u64)>,
    /// Biome 2.x: the source text, needed to convert `span` to line/column.
    #[serde(default, rename = "sourceCode")]
    pub source_code: Option<String>,
    /// Legacy shape (our fixtures + older biome): `start: {line, column}`.
    #[serde(default)]
    pub start: Option<Position>,
    #[serde(default)]
    pub end: Option<Position>,
}

/// Biome emits `location.path` in two shapes depending on version:
/// - string: `"path": "index.ts"`
/// - object: `"path": {"file": "/abs/foo.ts"}`
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum PathOrObject {
    Path(PathBuf),
    Object(serde_json::Value),
}

impl PathOrObject {
    pub fn display(&self) -> String {
        match self {
            PathOrObject::Path(p) => p.display().to_string(),
            PathOrObject::Object(v) => {
                if let Some(f) = v.get("file").and_then(|f| f.as_str()) {
                    return f.to_string();
                }
                v.to_string()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct Position {
    #[serde(default)]
    pub line: Option<u64>,
    #[serde(default)]
    pub column: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_EMPTY: &str = include_str!("../../tests/fixtures/biome_empty.json");
    const FIXTURE_WARNINGS: &str = include_str!("../../tests/fixtures/biome_warnings.json");
    const FIXTURE_ERRORS: &str = include_str!("../../tests/fixtures/biome_errors.json");
    const FIXTURE_PARSE_ERROR: &str = include_str!("../../tests/fixtures/biome_parse_error.json");
    const FIXTURE_V2_REAL: &str = include_str!("../../tests/fixtures/biome_v2_real.json");

    #[test]
    fn parses_empty_output() {
        let c: CheckOutput = serde_json::from_str(FIXTURE_EMPTY).unwrap();
        assert_eq!(c.summary.errors, 0);
        assert_eq!(c.summary.warnings, 0);
        assert!(c.diagnostics.is_empty());
    }

    #[test]
    fn parses_warnings_output() {
        let c: CheckOutput = serde_json::from_str(FIXTURE_WARNINGS).unwrap();
        assert!(c.summary.warnings > 0);
        assert!(!c.diagnostics.is_empty());
        assert_eq!(c.diagnostics[0].severity, Severity::Warning);
        // legacy fixture uses string message — must still flatten correctly
        assert_eq!(c.diagnostics[0].message, "This import is unused.");
    }

    #[test]
    fn parses_errors_output() {
        let c: CheckOutput = serde_json::from_str(FIXTURE_ERRORS).unwrap();
        assert!(c.summary.errors > 0);
        assert!(c.diagnostics.iter().any(|d| d.severity == Severity::Error));
    }

    #[test]
    fn parses_parse_error_output() {
        let c: CheckOutput = serde_json::from_str(FIXTURE_PARSE_ERROR).unwrap();
        assert!(c.summary.errors > 0);
    }

    #[test]
    fn extra_unknown_fields_do_not_break_parsing() {
        let raw = r#"{ "summary": { "errors": 0, "warnings": 0, "bogus_new_field": 42 }, "diagnostics": [], "unknownTopLevelField": true }"#;
        let c: CheckOutput = serde_json::from_str(raw).unwrap();
        assert_eq!(c.summary.errors, 0);
    }

    /// Real biome 2.x output — captured from biome 2.3.11. This is the
    /// shape that broke the hook in practice (message as segment array,
    /// location.path as {file: ...}, span as byte offsets).
    #[test]
    fn parses_biome_v2_real_output() {
        let c: CheckOutput = serde_json::from_str(FIXTURE_V2_REAL).unwrap();
        assert_eq!(c.summary.warnings, 2);
        assert_eq!(c.summary.changed, 1);
        assert_eq!(c.diagnostics.len(), 2);
        // message flattened from segment array
        assert_eq!(c.diagnostics[0].message, "This variable x is unused.");
        // path extracted from {file: ...} object
        let p = c.diagnostics[0]
            .location
            .as_ref()
            .unwrap()
            .path
            .as_ref()
            .unwrap();
        assert!(p.display().ends_with("test-formatting.ts"));
        // span present, start/end absent
        assert!(c.diagnostics[0].location.as_ref().unwrap().span.is_some());
        assert!(c.diagnostics[0].location.as_ref().unwrap().start.is_none());
    }

    #[test]
    fn flatten_message_handles_string_and_segments() {
        assert_eq!(
            flatten_message(serde_json::json!("plain string")),
            Some("plain string".to_string())
        );
        assert_eq!(
            flatten_message(serde_json::json!([
                {"elements": [], "content": "hello "},
                {"elements": ["Emphasis"], "content": "world"}
            ])),
            Some("hello world".to_string())
        );
        assert_eq!(flatten_message(serde_json::json!(null)), None);
        assert_eq!(flatten_message(serde_json::json!([])), None);
    }

    #[test]
    fn path_object_extracts_file_field() {
        let p = PathOrObject::Object(serde_json::json!({"file": "/abs/foo.ts"}));
        assert_eq!(p.display(), "/abs/foo.ts");
    }
}
