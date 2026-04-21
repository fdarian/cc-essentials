use serde::Deserialize;
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

#[derive(Debug, Clone, Deserialize)]
pub struct Diagnostic {
    #[serde(default)]
    pub severity: Severity,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub location: Option<Location>,
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
    #[serde(default)]
    pub start: Option<Position>,
    #[serde(default)]
    pub end: Option<Position>,
}

/// Biome sometimes emits location.path as a plain string, sometimes as
/// an object like `{ "file": "...", "path": "..." }`. Accept either.
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
            PathOrObject::Object(v) => v.to_string(),
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
}
