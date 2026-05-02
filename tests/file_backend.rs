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
use std::sync::Arc;

use host_setup::backends::file::File;
use host_setup::env::{Env, RunMode};
use host_setup::executor::Executor;
use host_setup::plan::Plan;
use host_setup::resource::{Changed, Resource};

fn dry_run_env() -> Env {
    Env::synthetic(false).with_run_mode(RunMode::DryRun)
}

#[tokio::test]
async fn writes_new_file_with_content_and_mode() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let path = tmp.path().join("nested/dir/managed.txt");
    let env = Env::synthetic(false);

    let resource = File {
        path: path.clone(),
        content: "hello\n".to_string(),
        mode: Some(Permissions::from_mode(0o600)),
        ..Default::default()
    };

    let changed = resource
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("first converge: {e}"));
    assert_eq!(
        changed,
        Changed::Yes,
        "first converge should be Changed::Yes"
    );

    let written = std::fs::read_to_string(&path).expect("read back written file");
    assert_eq!(written, "hello\n", "content mismatch: {written:?}");

    let mode = std::fs::metadata(&path)
        .expect("stat written file")
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(mode, 0o600, "permissions mismatch: 0o{mode:o}");
}

#[tokio::test]
async fn second_converge_is_unchanged_when_file_matches() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let path = tmp.path().join("idempotent.txt");
    let env = Env::synthetic(false);

    let resource = File {
        path: path.clone(),
        content: "stable\n".to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        ..Default::default()
    };

    let first = resource
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("first converge: {e}"));
    assert_eq!(first, Changed::Yes);

    let second = resource
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("second converge: {e}"));
    assert_eq!(
        second,
        Changed::No,
        "second converge of identical desired state should report Changed::No",
    );
}

#[tokio::test]
async fn modified_content_triggers_rewrite() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let path = tmp.path().join("rewrite.txt");
    let env = Env::synthetic(false);

    let initial = File {
        path: path.clone(),
        content: "v1\n".to_string(),
        ..Default::default()
    };
    initial
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("write v1: {e}"));

    let updated = File {
        path: path.clone(),
        content: "v2\n".to_string(),
        ..Default::default()
    };
    let result = updated
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("write v2: {e}"));
    assert_eq!(result, Changed::Yes);

    let on_disk = std::fs::read_to_string(&path).expect("read back");
    assert_eq!(on_disk, "v2\n", "expected v2 content; got {on_disk:?}");
}

#[tokio::test]
async fn dry_run_does_not_create_missing_file() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let path = tmp.path().join("dry-run-new.txt");
    let env = dry_run_env();

    let resource = File {
        path: path.clone(),
        content: "would have been written\n".to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        ..Default::default()
    };

    let result = resource
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("dry-run converge failed: {e}"));
    assert_eq!(
        result,
        Changed::Yes,
        "dry-run on a missing file should still report Changed::Yes",
    );
    assert!(
        !path.exists(),
        "dry-run must not create the file at {}",
        path.display(),
    );
}

#[tokio::test]
async fn dry_run_does_not_overwrite_existing_file() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let path = tmp.path().join("dry-run-existing.txt");
    std::fs::write(&path, "ORIGINAL\n").expect("seed file");

    let env = dry_run_env();
    let resource = File {
        path: path.clone(),
        content: "REPLACEMENT\n".to_string(),
        ..Default::default()
    };

    let result = resource
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("dry-run converge failed: {e}"));
    assert_eq!(
        result,
        Changed::Yes,
        "dry-run on a file with different content should report Changed::Yes",
    );

    let on_disk = std::fs::read_to_string(&path).expect("read back");
    assert_eq!(
        on_disk, "ORIGINAL\n",
        "dry-run must not overwrite existing content; got {on_disk:?}",
    );
}

#[tokio::test]
async fn dry_run_reports_unchanged_when_file_already_matches() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let path = tmp.path().join("dry-run-already-good.txt");
    std::fs::write(&path, "exactly right\n").expect("seed file");

    let env = dry_run_env();
    let resource = File {
        path: path.clone(),
        content: "exactly right\n".to_string(),
        ..Default::default()
    };

    let result = resource
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("dry-run converge failed: {e}"));
    assert_eq!(
        result,
        Changed::No,
        "dry-run on an in-sync file must report Changed::No, not Changed::Yes",
    );
}

#[tokio::test]
async fn file_is_runnable_through_executor() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let path = tmp.path().join("via-executor.txt");
    let env = Arc::new(Env::synthetic(false));

    let mut plan = Plan::new();
    plan.add(File {
        path: path.clone(),
        content: "executor-managed\n".to_string(),
        ..Default::default()
    });

    let report = Executor::new(plan)
        .run(env)
        .await
        .unwrap_or_else(|e| panic!("executor run: {e}"));
    assert_eq!(report.outcomes.len(), 1, "expected one outcome");
    assert_eq!(report.outcomes[0].changed, Changed::Yes);

    let written = std::fs::read_to_string(&path).expect("read back");
    assert_eq!(written, "executor-managed\n");
}

#[tokio::test]
async fn executor_in_dry_run_does_not_touch_disk() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let path = tmp.path().join("via-executor-dry.txt");
    let env = Arc::new(dry_run_env());

    let mut plan = Plan::new();
    plan.add(File {
        path: path.clone(),
        content: "should-not-land\n".to_string(),
        ..Default::default()
    });

    let report = Executor::new(plan)
        .run(env)
        .await
        .unwrap_or_else(|e| panic!("executor dry-run: {e}"));
    assert_eq!(report.outcomes.len(), 1);
    assert_eq!(
        report.outcomes[0].changed,
        Changed::Yes,
        "dry-run reports Changed::Yes for resources that would have changed",
    );
    assert!(
        !path.exists(),
        "dry-run must leave disk untouched at {}",
        path.display(),
    );
}
