use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn log_mode_accepts_valid_command_stream_and_writes_redacted_log() {
    let dir = tempdir().expect("tempdir");
    let log = dir.path().join("step.log");
    let redacted = dir.path().join("redacted.log");
    std::fs::write(
        &log,
        "::add-mask::s3cr3t\nplain s3cr3t\n::warning::s3cr3t\n",
    )
    .expect("write log");

    Command::cargo_bin("gha-command-proof")
        .expect("binary")
        .args([
            "log",
            log.to_str().unwrap(),
            "--redacted-log-output",
            redacted.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("summary:"));

    let redacted = std::fs::read_to_string(redacted).expect("read redacted");
    assert!(redacted.contains("plain ***"));
    assert!(!redacted.contains("s3cr3t"));
}

#[test]
fn log_mode_fails_disabled_stdout_commands() {
    let dir = tempdir().expect("tempdir");
    let log = dir.path().join("bad.log");
    std::fs::write(&log, "::set-env name=FOO::bar\n").expect("write log");

    Command::cargo_bin("gha-command-proof")
        .expect("binary")
        .args(["log", log.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("commands.unsupported"));
}

#[test]
fn env_file_mode_fails_node_options() {
    let dir = tempdir().expect("tempdir");
    let env = dir.path().join("GITHUB_ENV");
    std::fs::write(&env, "NODE_OPTIONS=--inspect\n").expect("write env");

    Command::cargo_bin("gha-command-proof")
        .expect("binary")
        .args(["env-file", "--kind", "env", env.to_str().unwrap()])
        .assert()
        .failure()
        .stdout(predicate::str::contains("env_file.env.node_options"));
}

#[test]
fn step_mode_detects_masked_output_value() {
    let dir = tempdir().expect("tempdir");
    let log = dir.path().join("step.log");
    let output = dir.path().join("GITHUB_OUTPUT");
    std::fs::write(&log, "::add-mask::s3cr3t\n").expect("write log");
    std::fs::write(&output, "token=s3cr3t\n").expect("write output");

    Command::cargo_bin("gha-command-proof")
        .expect("binary")
        .args([
            "step",
            "--log",
            log.to_str().unwrap(),
            "--github-output",
            output.to_str().unwrap(),
            "--format",
            "json",
        ])
        .assert()
        .failure()
        .stdout(predicate::str::contains("env_file.output.masked_value"))
        .stdout(predicate::str::contains("\"value\": \"***\""))
        .stdout(predicate::str::contains("s3cr3t").not());
}

#[test]
fn strict_mode_treats_warnings_as_failures() {
    let dir = tempdir().expect("tempdir");
    let log = dir.path().join("warn.log");
    std::fs::write(&log, "::set-output name=result::ok\n").expect("write log");

    Command::cargo_bin("gha-command-proof")
        .expect("binary")
        .args(["log", log.to_str().unwrap(), "--strict"])
        .assert()
        .failure()
        .stdout(predicate::str::contains("0 warned, 1 failed"));
}
