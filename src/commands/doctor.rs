use crate::cache::Cache;
use crate::detect::{self, DetectedProject};
use anyhow::{Context, Result};
use owo_colors::OwoColorize;
use std::io::Write;
use std::path::Path;

/// Render the doctor report into `out`. Used by both the CLI and snapshot tests.
pub fn run(start: &Path, cache: &Cache, out: &mut dyn Write, use_color: bool) -> Result<()> {
    let detected = detect::detect_from(start, cache).context("run detection")?;
    render(&detected, cache, out, use_color)
}

fn render(d: &DetectedProject, cache: &Cache, out: &mut dyn Write, use_color: bool) -> Result<()> {
    // Helpers that conditionally apply color so snapshot tests can run without ANSI codes.
    let header = |s: &str| -> String {
        if use_color {
            s.bold().cyan().to_string()
        } else {
            s.to_string()
        }
    };
    let ok = |s: &str| -> String {
        if use_color {
            s.green().to_string()
        } else {
            s.to_string()
        }
    };
    let warn = |s: &str| -> String {
        if use_color {
            s.yellow().to_string()
        } else {
            s.to_string()
        }
    };

    writeln!(out, "{}", header("cc-essentials doctor"))?;
    writeln!(out, "  start: {}", d.start.display())?;

    match &d.repo_root {
        Some(root) => writeln!(out, "  git repo root: {} {}", root.display(), ok("(found)"))?,
        None => writeln!(out, "  git repo root: {}", warn("not in a git repo"))?,
    }

    match &d.package_manager {
        Some(pm) => writeln!(
            out,
            "  package manager: {:?} ({}) {}",
            pm.0,
            pm.1.display(),
            ok("(found)")
        )?,
        None => writeln!(
            out,
            "  package manager: {}",
            warn("no lockfile found (not a JS/TS project?)")
        )?,
    }

    match &d.biome {
        Some(b) => {
            writeln!(
                out,
                "  biome config: {} {}",
                b.config_path.display(),
                ok("(found)")
            )?;
            writeln!(
                out,
                "  biome binary: {} {}",
                b.binary_path.display(),
                ok("(found)")
            )?;
            writeln!(out, "  biome version: {}", b.version)?;
        }
        None => {
            writeln!(
                out,
                "  biome config: {}",
                warn("not configured (no biome.json / biome.jsonc found)")
            )?;
            writeln!(out, "  biome binary: {}", warn("not applicable"))?;
        }
    }

    writeln!(out, "  cache dir: {}", cache.dir().display())?;

    // Brief summary line so the user sees at-a-glance status.
    if d.biome.is_some() {
        writeln!(out, "{}", ok("ready: biome detected"))?;
    } else if d.package_manager.is_some() {
        writeln!(
            out,
            "{}",
            warn("partial: JS/TS project detected but no biome config")
        )?;
    } else {
        writeln!(
            out,
            "{}",
            warn("not a supported project (no JS/TS lockfile, no biome config)")
        )?;
    }

    Ok(())
}
