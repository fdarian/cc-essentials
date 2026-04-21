use serde::Serialize;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize)]
pub struct LogEntry<'a> {
    pub ts: u128,
    pub event: &'a str,
    #[serde(flatten)]
    pub fields: serde_json::Value,
}

/// Append `event` + `fields` as one JSONL line to `path`. Best-effort: all
/// failures are swallowed. Only writes if `CC_ESSENTIALS_LOG=1`.
pub fn log_event(cache_dir: &Path, event: &str, fields: serde_json::Value) {
    if std::env::var("CC_ESSENTIALS_LOG").as_deref() != Ok("1") {
        return;
    }
    let path = cache_dir.join("hooks.log");
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let entry = LogEntry { ts, event, fields };
    let Ok(line) = serde_json::to_string(&entry) else {
        return;
    };
    let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    else {
        return;
    };
    let _ = writeln!(f, "{line}");
}
