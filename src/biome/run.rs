use super::schema::CheckOutput;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Outcome of shelling out to biome.
#[derive(Debug)]
pub enum BiomeOutcome {
    /// Biome emitted JSON we successfully parsed.
    Parsed {
        check: CheckOutput,
        /// The file we asked biome to check, as passed on the command line (relative to `cwd`).
        relative_file: PathBuf,
        exit_code: Option<i32>,
    },
    /// Biome ran but we couldn't parse its JSON output. Preserves stderr/stdout for diagnostics.
    FallbackText {
        stdout: String,
        stderr: String,
        exit_code: Option<i32>,
    },
    /// Biome failed to spawn (e.g. binary vanished between detection and invocation).
    SpawnFailed { error: String },
}

/// Run `<binary> check --write --reporter=json <relative_file>` with cwd = `config_dir`.
///
/// `config_dir` MUST be the directory containing the biome.json config that should govern the run
/// (biome resolves config from cwd, not from the file path, so monorepo per-package configs only
/// work if we cd to the right dir).
///
/// `relative_file` MUST be expressible relative to `config_dir`. Callers are responsible for making
/// it relative.
pub fn run_check(binary: &Path, config_dir: &Path, relative_file: &Path) -> BiomeOutcome {
    let out: Output = match Command::new(binary)
        .current_dir(config_dir)
        .args(["check", "--write", "--reporter=json"])
        .arg(relative_file)
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            return BiomeOutcome::SpawnFailed {
                error: e.to_string(),
            };
        }
    };

    let exit_code = out.status.code();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    match serde_json::from_str::<CheckOutput>(&stdout) {
        Ok(check) => BiomeOutcome::Parsed {
            check,
            relative_file: relative_file.to_path_buf(),
            exit_code,
        },
        Err(_) => BiomeOutcome::FallbackText {
            stdout,
            stderr,
            exit_code,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    fn make_exec(p: &Path, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::write(p, body).unwrap();
        let mut perms = fs::metadata(p).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(p, perms).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn parses_stub_biome_output() {
        let t = tempfile::tempdir().unwrap();
        let bin = t.path().join("biome");
        let fixture = include_str!("../../tests/fixtures/biome_warnings.json");
        make_exec(&bin, &format!("#!/bin/sh\ncat <<'EOF'\n{fixture}\nEOF"));

        let outcome = run_check(&bin, t.path(), Path::new("index.ts"));
        match outcome {
            BiomeOutcome::Parsed {
                check,
                relative_file,
                ..
            } => {
                assert_eq!(relative_file, Path::new("index.ts"));
                assert_eq!(check.summary.warnings, 2);
            }
            other => panic!("expected Parsed, got {other:?}"),
        }
    }

    #[test]
    #[cfg(unix)]
    fn falls_back_to_text_when_output_is_not_json() {
        let t = tempfile::tempdir().unwrap();
        let bin = t.path().join("biome");
        make_exec(
            &bin,
            "#!/bin/sh\necho 'biome ran into an internal error' 1>&2\nexit 1\n",
        );

        let outcome = run_check(&bin, t.path(), Path::new("foo.ts"));
        match outcome {
            BiomeOutcome::FallbackText {
                stderr, exit_code, ..
            } => {
                assert!(stderr.contains("internal error"));
                assert_eq!(exit_code, Some(1));
            }
            other => panic!("expected FallbackText, got {other:?}"),
        }
    }

    #[test]
    #[cfg(unix)]
    fn spawn_failed_when_binary_missing() {
        let missing = Path::new("/nonexistent/biome-binary-does-not-exist");
        let outcome = run_check(missing, Path::new("/tmp"), Path::new("foo.ts"));
        match outcome {
            BiomeOutcome::SpawnFailed { .. } => {}
            other => panic!("expected SpawnFailed, got {other:?}"),
        }
    }

    #[test]
    #[cfg(unix)]
    fn passes_cwd_and_args_through() {
        // Stub biome that dumps its cwd and argv and exits 0 with JSON on stdout.
        let t = tempfile::tempdir().unwrap();
        let bin = t.path().join("biome");
        let marker = t.path().join("marker.txt");
        let script = format!(
            "#!/bin/sh\npwd > {marker_display}\necho \"$@\" >> {marker_display}\ncat <<'EOF'\n{{\"summary\":{{\"errors\":0,\"warnings\":0}},\"diagnostics\":[],\"command\":\"check\"}}\nEOF\n",
            marker_display = marker.display()
        );
        make_exec(&bin, &script);

        let workdir = t.path().join("work");
        fs::create_dir_all(&workdir).unwrap();
        let outcome = run_check(&bin, &workdir, Path::new("hello.ts"));
        assert!(matches!(outcome, BiomeOutcome::Parsed { .. }));

        let recorded = fs::read_to_string(&marker).unwrap();
        // pwd under macOS may be `/private/var/...` vs symlink; just assert endswith("work\n...")
        assert!(
            recorded.lines().next().unwrap().ends_with("/work"),
            "pwd={recorded}"
        );
        assert!(recorded.contains("check --write --reporter=json hello.ts"));
    }
}
