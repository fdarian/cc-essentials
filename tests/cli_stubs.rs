use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn doctor_stub_runs() {
    Command::cargo_bin("cc-essentials")
        .unwrap()
        .arg("doctor")
        .assert()
        .success()
        .stdout(contains("doctor"));
}

#[test]
fn hooks_crite_exits_0_on_empty_stdin() {
    Command::cargo_bin("cc-essentials")
        .unwrap()
        .args(["hooks", "crite"])
        .write_stdin("")
        .assert()
        .success();
}

#[test]
fn help_works() {
    Command::cargo_bin("cc-essentials")
        .unwrap()
        .arg("--help")
        .assert()
        .success();
}
