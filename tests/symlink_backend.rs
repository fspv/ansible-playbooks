#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::missing_const_for_fn,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc
)]

use std::path::PathBuf;

use host_setup::backends::symlink::Symlink;
use host_setup::env::{Env, RunMode};
use host_setup::resource::{Changed, Resource};

#[tokio::test]
async fn creates_a_new_symlink() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("target.txt");
    std::fs::write(&target, "hi").expect("seed target");
    let link = tmp.path().join("link");

    let env = Env::synthetic(false);
    let result = Symlink {
        path: link.clone(),
        target: target.clone(),
        ..Default::default()
    }
    .converge_one(&env)
    .await
    .unwrap_or_else(|e| panic!("first converge: {e}"));
    assert_eq!(result, Changed::Yes);

    let resolved = std::fs::read_link(&link).expect("read_link");
    assert_eq!(resolved, target);
}

#[tokio::test]
async fn second_converge_is_unchanged_when_target_matches() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("target");
    std::fs::write(&target, "x").expect("seed");
    let link = tmp.path().join("link");

    let env = Env::synthetic(false);
    let resource = Symlink {
        path: link,
        target,
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
async fn replaces_symlink_pointing_elsewhere() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let old_target = tmp.path().join("old");
    let new_target = tmp.path().join("new");
    std::fs::write(&old_target, "old").expect("seed old");
    std::fs::write(&new_target, "new").expect("seed new");
    let link = tmp.path().join("link");
    std::os::unix::fs::symlink(&old_target, &link).expect("seed symlink");

    let env = Env::synthetic(false);
    let result = Symlink {
        path: link.clone(),
        target: new_target.clone(),
        ..Default::default()
    }
    .converge_one(&env)
    .await
    .unwrap_or_else(|e| panic!("converge: {e}"));
    assert_eq!(result, Changed::Yes);
    assert_eq!(std::fs::read_link(&link).expect("read_link"), new_target);
}

#[tokio::test]
async fn refuses_to_clobber_regular_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("not-a-symlink");
    std::fs::write(&path, "real file").expect("seed");

    let env = Env::synthetic(false);
    let err = Symlink {
        path: path.clone(),
        target: PathBuf::from("/dev/null"),
        ..Default::default()
    }
    .converge_one(&env)
    .await
    .expect_err("must refuse to overwrite a regular file");
    let msg = err.to_string();
    assert!(
        msg.contains("not a symlink"),
        "error should mention non-symlink; got: {msg}",
    );
    let still_a_file = std::fs::read_to_string(&path).expect("read");
    assert_eq!(still_a_file, "real file", "file content must be untouched");
}

#[tokio::test]
async fn dry_run_writes_no_symlink() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let link = tmp.path().join("dry-link");
    let env = Env::synthetic(false).with_run_mode(RunMode::DryRun);

    let result = Symlink {
        path: link.clone(),
        target: PathBuf::from("/dev/null"),
        ..Default::default()
    }
    .converge_one(&env)
    .await
    .unwrap_or_else(|e| panic!("dry-run: {e}"));
    assert_eq!(result, Changed::Yes);
    assert!(
        !link.exists() && std::fs::symlink_metadata(&link).is_err(),
        "dry-run must not create the symlink",
    );
}
