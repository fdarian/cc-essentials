use regex::Regex;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Find a biome binary.
///
/// Search order:
/// 1. Walk up from `start` (directory), checking for `node_modules/.bin/biome` at each level.
/// 2. Fall back to `which::which("biome")`.
///
/// Returns `Ok(None)` when biome is not installed in either location.
#[allow(dead_code)] // wired up in future commit
pub fn find_biome_binary(start: &Path) -> io::Result<Option<PathBuf>> {
    let mut current = start;
    loop {
        let probe = current.join("node_modules").join(".bin").join("biome");
        if probe.exists() {
            return Ok(Some(probe));
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    match which::which("biome") {
        Ok(p) => Ok(Some(p)),
        Err(which::Error::CannotFindBinaryPath) => Ok(None),
        Err(e) => Err(io::Error::other(format!("which biome: {e}"))),
    }
}

#[allow(dead_code)] // used by probe_version
fn semver_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\d+\.\d+\.\d+").unwrap())
}

/// Run `<binary> --version` and extract the first semver-shaped token.
#[allow(dead_code)] // wired up in future commit
pub fn probe_version(binary: &Path) -> io::Result<String> {
    let output = Command::new(binary).arg("--version").output()?;
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    match semver_re().find(&combined) {
        Some(m) => Ok(m.as_str().to_string()),
        None => Err(io::Error::other(format!(
            "could not parse biome version from: {}",
            combined.trim()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    fn make_exec_script(path: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, body).unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn finds_nearest_node_modules_bin() {
        let t = tempfile::tempdir().unwrap();
        let bin_dir = t.path().join("node_modules/.bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let bin = bin_dir.join("biome");
        make_exec_script(&bin, "#!/bin/sh\necho dummy\n");

        let child = t.path().join("src");
        fs::create_dir(&child).unwrap();
        let got = find_biome_binary(&child).unwrap().unwrap();
        assert_eq!(got, bin);
    }

    #[test]
    #[cfg(unix)]
    fn nested_node_modules_wins_over_ancestor() {
        let t = tempfile::tempdir().unwrap();
        let outer_bin = t.path().join("node_modules/.bin");
        fs::create_dir_all(&outer_bin).unwrap();
        make_exec_script(&outer_bin.join("biome"), "#!/bin/sh\necho outer\n");

        let inner = t.path().join("packages/foo");
        fs::create_dir_all(&inner).unwrap();
        let inner_bin = inner.join("node_modules/.bin");
        fs::create_dir_all(&inner_bin).unwrap();
        make_exec_script(&inner_bin.join("biome"), "#!/bin/sh\necho inner\n");

        let got = find_biome_binary(&inner).unwrap().unwrap();
        assert_eq!(got, inner_bin.join("biome"));
    }

    #[test]
    #[cfg(unix)]
    fn probe_version_extracts_semver() {
        let t = tempfile::tempdir().unwrap();
        let bin = t.path().join("biome");
        make_exec_script(&bin, "#!/bin/sh\necho 'Version: 1.9.4'\n");

        let v = probe_version(&bin).unwrap();
        assert_eq!(v, "1.9.4");
    }

    #[test]
    #[cfg(unix)]
    fn probe_version_errors_on_unparseable_output() {
        let t = tempfile::tempdir().unwrap();
        let bin = t.path().join("biome");
        make_exec_script(&bin, "#!/bin/sh\necho hello world\n");

        assert!(probe_version(&bin).is_err());
    }
}
