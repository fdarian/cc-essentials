use crate::cache::Cache;
use anyhow::Result;
use owo_colors::OwoColorize;
use std::io::Write;

pub fn run(cache: &Cache, out: &mut dyn Write, use_color: bool) -> Result<()> {
    let header = |s: &str| -> String {
        if use_color {
            s.bold().cyan().to_string()
        } else {
            s.to_string()
        }
    };

    let log_enabled = std::env::var("CC_ESSENTIALS_LOG").as_deref() == Ok("1");
    if log_enabled {
        writeln!(out, "log enabled: yes")?;
    } else {
        writeln!(out, "log enabled: no (set CC_ESSENTIALS_LOG=1)")?;
    }

    let log_file = cache.dir().join("hooks.log");
    writeln!(out, "log file: {}", log_file.display())?;

    let last_error_file = cache.dir().join("last-error.json");
    writeln!(out, "last error: {}", last_error_file.display())?;

    // Show last-error.json if it exists.
    match std::fs::read_to_string(&last_error_file) {
        Ok(contents) => {
            writeln!(out, "{}", header("--- last error ---"))?;
            match serde_json::from_str::<serde_json::Value>(&contents) {
                Ok(v) => {
                    let pretty = serde_json::to_string_pretty(&v).unwrap_or(contents);
                    writeln!(out, "{pretty}")?;
                }
                Err(_) => writeln!(out, "{contents}")?,
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            writeln!(out, "no last error recorded")?;
        }
        Err(e) => {
            writeln!(out, "could not read last-error.json: {e}")?;
        }
    }

    // Show last 10 lines of hooks.log if it exists.
    match std::fs::read_to_string(&log_file) {
        Ok(contents) => {
            writeln!(out, "{}", header("--- recent events ---"))?;
            let lines: Vec<&str> = contents.split('\n').filter(|l| !l.is_empty()).collect();
            let tail = if lines.len() > 10 {
                &lines[lines.len() - 10..]
            } else {
                lines.as_slice()
            };
            for line in tail {
                match serde_json::from_str::<serde_json::Value>(line) {
                    Ok(v) => {
                        let pretty =
                            serde_json::to_string_pretty(&v).unwrap_or_else(|_| line.to_string());
                        writeln!(out, "{pretty}")?;
                    }
                    Err(_) => writeln!(out, "{line}")?,
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            writeln!(
                out,
                "no log entries (logging disabled or no invocations yet)"
            )?;
        }
        Err(e) => {
            writeln!(out, "could not read hooks.log: {e}")?;
        }
    }

    Ok(())
}
