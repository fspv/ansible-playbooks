#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::missing_const_for_fn,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc
)]

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

use host_setup::backends::directory::Directory;
use host_setup::env::{Env, RunMode};
use host_setup::resource::{Changed, Resource};

#[tokio::test]
async fn creates_a_new_directory_with_mode() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("nested/created");

    let env = Env::synthetic(false);
    let result = Directory {
        path: path.clone(),
        mode: Some(Permissions::from_mode(0o750)),
        ..Default::default()
    }
    .converge_one(&env)
    .await
    .unwrap_or_else(|e| panic!("first converge: {e}"));
    assert_eq!(result, Changed::Yes);

    let meta = std::fs::metadata(&path).expect("stat");
    assert!(meta.is_dir(), "expected directory");
    assert_eq!(meta.permissions().mode() & 0o7777, 0o750);
}

#[tokio::test]
async fn second_converge_is_unchanged_when_directory_matches() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("idempotent");

    let env = Env::synthetic(false);
    let resource = Directory {
        path: path.clone(),
        mode: Some(Permissions::from_mode(0o755)),
        ..Default::default()
    };
    resource
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("first: {e}"));
    let second = resource
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("second: {e}"));
    assert_eq!(second, Changed::No);
}

#[tokio::test]
async fn mode_drift_is_repaired() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("driftee");
    std::fs::create_dir(&path).expect("seed dir");
    std::fs::set_permissions(&path, Permissions::from_mode(0o700)).expect("seed perms");

    let env = Env::synthetic(false);
    let result = Directory {
        path: path.clone(),
        mode: Some(Permissions::from_mode(0o755)),
        ..Default::default()
    }
    .converge_one(&env)
    .await
    .unwrap_or_else(|e| panic!("converge: {e}"));
    assert_eq!(result, Changed::Yes);
    let mode = std::fs::metadata(&path).expect("stat").permissions().mode() & 0o7777;
    assert_eq!(mode, 0o755);
}

#[tokio::test]
async fn refuses_when_path_is_a_regular_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("a-file");
    std::fs::write(&path, "not a dir").expect("seed file");

    let env = Env::synthetic(false);
    let err = Directory {
        path: path.clone(),
        ..Default::default()
    }
    .converge_one(&env)
    .await
    .expect_err("must refuse non-directory at target path");
    let msg = err.to_string();
    assert!(
        msg.contains("not a directory"),
        "expected 'not a directory' in error; got: {msg}",
    );
    let still_a_file = std::fs::read_to_string(&path).expect("read");
    assert_eq!(still_a_file, "not a dir", "file content must be untouched");
}

#[tokio::test]
async fn dry_run_creates_nothing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("would-have-been-here");

    let env = Env::synthetic(false).with_run_mode(RunMode::DryRun);
    let result = Directory {
        path: path.clone(),
        mode: Some(Permissions::from_mode(0o755)),
        ..Default::default()
    }
    .converge_one(&env)
    .await
    .unwrap_or_else(|e| panic!("dry-run: {e}"));
    assert_eq!(result, Changed::Yes);
    assert!(!path.exists(), "dry-run must not create the directory");
}
