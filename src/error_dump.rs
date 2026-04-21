use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
pub struct LastErrorPayload {
    pub ts_unix_ms: u128,
    pub event: &'static str, // "fallback_text" | "spawn_failed"
    pub tool_name: String,
    pub file_path: String,
    pub biome_binary: Option<PathBuf>,
    pub biome_config: Option<PathBuf>,
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub exit_code: Option<i32>,
    pub stdout_first_4k: String,
    pub stderr_first_4k: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Truncate a string to at most `max_bytes`, snapping down to the last
/// complete UTF-8 char boundary. Returns an owned String.
pub fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Write a last-error dump to `<cache_dir>/last-error.json` atomically.
/// All failures are swallowed — the hook's exit-0 invariant is non-negotiable.
pub fn write_last_error(cache_dir: &Path, payload: &LastErrorPayload) {
    if let Err(e) = write_last_error_inner(cache_dir, payload) {
        eprintln!("[cc-essentials] write_last_error failed: {e}");
    }
}

fn write_last_error_inner(cache_dir: &Path, payload: &LastErrorPayload) -> Result<()> {
    let target = cache_dir.join("last-error.json");
    let bytes = serde_json::to_vec_pretty(payload)?;
    let mut tmp = tempfile::NamedTempFile::new_in(cache_dir)?;
    std::io::Write::write_all(&mut tmp, &bytes)?;
    tmp.persist(&target)
        .map_err(|e| anyhow::anyhow!("persist last-error.json: {}", e))?;
    Ok(())
}

pub fn current_ts_unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_utf8_shortens_long_ascii() {
        let s = "a".repeat(10000);
        let result = truncate_utf8(&s, 4096);
        assert_eq!(result.len(), 4096);
        assert!(result.chars().all(|c| c == 'a'));
    }

    #[test]
    fn truncate_utf8_respects_utf8_boundaries() {
        // "日" is 3 bytes in UTF-8
        let s = "日本語".repeat(500);
        let result = truncate_utf8(&s, 10);
        // must be valid UTF-8 and <= 10 bytes
        assert!(result.len() <= 10);
        // 9 = 3 chars * 3 bytes each
        assert_eq!(result.len(), 9);
        assert!(result.chars().count() == 3);
    }

    #[test]
    fn truncate_utf8_no_truncation_when_within_limit() {
        let s = "hello";
        assert_eq!(truncate_utf8(s, 100), "hello");
    }

    #[test]
    fn write_last_error_round_trip() {
        let t = tempfile::tempdir().unwrap();
        let payload = LastErrorPayload {
            ts_unix_ms: 1713123456789,
            event: "fallback_text",
            tool_name: "Write".to_string(),
            file_path: "/abs/path/foo.ts".to_string(),
            biome_binary: Some(PathBuf::from("/node_modules/.bin/biome")),
            biome_config: Some(PathBuf::from("/biome.json")),
            argv: vec!["check".to_string(), "--write".to_string()],
            cwd: Some(PathBuf::from("/project")),
            exit_code: Some(0),
            stdout_first_4k: "garbage output".to_string(),
            stderr_first_4k: String::new(),
            error: None,
        };

        write_last_error(t.path(), &payload);

        let contents = std::fs::read_to_string(t.path().join("last-error.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(v["event"], "fallback_text");
        assert_eq!(v["tool_name"], "Write");
        assert_eq!(v["ts_unix_ms"], 1713123456789_u64);
        assert_eq!(v["stdout_first_4k"], "garbage output");
        assert!(
            v.get("error").is_none(),
            "error should be omitted when None"
        );
    }
}
