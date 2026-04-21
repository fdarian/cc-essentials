use assert_cmd::Command;
use cc_essentials::cache::Cache;
use cc_essentials::commands::doctor;
use insta::assert_snapshot;
use std::fs;
use std::path::Path;

fn render(start: &Path) -> String {
    let cache_dir = start.join("__cache__");
    let cache = Cache::open_at(cache_dir).unwrap();
    let mut out: Vec<u8> = Vec::new();
    doctor::run(start, &cache, &mut out, false).unwrap();
    let s = String::from_utf8(out).unwrap();
    // Canonicalize start so the replacement matches the paths written by detect_from
    // (which also canonicalizes). On macOS, /var -> /private/var.
    let canonical = std::fs::canonicalize(start).unwrap();
    // Redact canonical form first (longer match), then raw form as fallback
    // so both the detection output and the cache dir path are stable.
    let s = s.replace(canonical.to_string_lossy().as_ref(), "<TMP>");
    s.replace(start.to_string_lossy().as_ref(), "<TMP>")
}

#[cfg(unix)]
fn make_exec(p: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(p, body).unwrap();
    let mut perms = fs::metadata(p).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(p, perms).unwrap();
}

#[test]
fn doctor_empty_tempdir_reports_nothing_detected() {
    let t = tempfile::tempdir().unwrap();
    let out = render(t.path());
    assert_snapshot!("empty", out);
}

#[test]
fn doctor_git_only_reports_git_but_no_js() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir(t.path().join(".git")).unwrap();
    let out = render(t.path());
    assert_snapshot!("git_only", out);
}

#[test]
#[cfg(unix)]
fn doctor_full_stack_biome_pnpm() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir(t.path().join(".git")).unwrap();
    fs::write(t.path().join("pnpm-lock.yaml"), "").unwrap();
    fs::write(t.path().join("biome.json"), "{}").unwrap();
    let bin_dir = t.path().join("node_modules/.bin");
    fs::create_dir_all(&bin_dir).unwrap();
    make_exec(&bin_dir.join("biome"), "#!/bin/sh\necho 'Version: 1.9.4'\n");
    let out = render(t.path());
    assert_snapshot!("full_stack", out);
}

#[test]
fn doctor_lockfile_without_biome() {
    let t = tempfile::tempdir().unwrap();
    fs::create_dir(t.path().join(".git")).unwrap();
    fs::write(t.path().join("package-lock.json"), "").unwrap();
    let out = render(t.path());
    assert_snapshot!("lockfile_no_biome", out);
}

#[test]
fn doctor_cli_invocation_exits_0() {
    Command::cargo_bin("cc-essentials")
        .unwrap()
        .arg("doctor")
        .assert()
        .success();
}
