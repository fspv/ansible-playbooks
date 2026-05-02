#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::missing_const_for_fn,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc
)]

use host_setup::backends::command::Command;
use host_setup::env::{Env, RunMode};
use host_setup::resource::{Changed, Resource};

#[tokio::test]
async fn dry_run_does_not_execute_command() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let marker = tmp.path().join("would-have-run.marker");
    let env = Env::synthetic(false).with_run_mode(RunMode::DryRun);

    let cmd = Command {
        name: "create marker".to_string(),
        argv: vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            format!("touch {}", marker.display()),
        ],
        ..Default::default()
    };

    let result = cmd
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("dry-run converge failed: {e}"));
    assert_eq!(
        result,
        Changed::Yes,
        "dry-run command must report Changed::Yes",
    );
    assert!(
        !marker.exists(),
        "dry-run must not have executed the command; marker exists at {}",
        marker.display(),
    );
}

#[tokio::test]
async fn apply_mode_runs_command_successfully() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let marker = tmp.path().join("did-run.marker");
    let env = Env::synthetic(false);

    let cmd = Command {
        name: "create marker".to_string(),
        argv: vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            format!("touch {}", marker.display()),
        ],
        ..Default::default()
    };

    let result = cmd
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("apply converge failed: {e}"));
    assert_eq!(result, Changed::Yes);
    assert!(
        marker.exists(),
        "apply must have created the marker at {}",
        marker.display(),
    );
}

#[tokio::test]
async fn nonzero_exit_returns_error() {
    let env = Env::synthetic(false);

    let cmd = Command {
        name: "fail".to_string(),
        argv: vec![
            "/bin/sh".to_string(),
            "-c".to_string(),
            "exit 7".to_string(),
        ],
        ..Default::default()
    };

    let err = cmd
        .converge_one(&env)
        .await
        .expect_err("expected non-zero exit to surface as error");
    let s = err.to_string();
    assert!(
        s.contains("exited with"),
        "error message should mention exit code; got: {s}",
    );
}

#[tokio::test]
async fn empty_argv_returns_error() {
    let env = Env::synthetic(false);

    let cmd = Command {
        name: "broken".to_string(),
        argv: Vec::new(),
        ..Default::default()
    };

    let err = cmd
        .converge_one(&env)
        .await
        .expect_err("empty argv must error");
    let s = err.to_string();
    assert!(
        s.contains("empty argv"),
        "error message should explain empty argv; got: {s}",
    );
}
