// e2e tests for the User backend. These mutate /etc/passwd and /etc/group, so
// they only run inside a container with root. Skipped by default; run via
// `cargo test --locked -- --include-ignored` from a Dockerfile-built image.
// Each test calls assert_in_container() so accidentally running with
// --include-ignored on the host fails loudly instead of corrupting state.

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
use std::process::Command;

use host_setup::backends::user::User;
use host_setup::env::Env;
use host_setup::resource::{Changed, Resource};

fn assert_in_container() {
    let env = Env::detect().unwrap_or_else(|e| panic!("Env::detect failed: {e}"));
    assert!(
        env.is_container(),
        "user e2e tests must run inside a container — invoke `cargo test --locked -- --include-ignored` from a Dockerfile-built image, not on the host",
    );
}

fn user_exists(name: &str) -> bool {
    Command::new("getent")
        .args(["passwd", name])
        .output()
        .expect("getent passwd")
        .status
        .success()
}

fn group_exists(name: &str) -> bool {
    Command::new("getent")
        .args(["group", name])
        .output()
        .expect("getent group")
        .status
        .success()
}

fn supplementary_groups(name: &str) -> Vec<String> {
    let output = Command::new("id")
        .args(["-nG", name])
        .output()
        .expect("id -nG");
    assert!(output.status.success(), "id -nG failed for {name}");
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(String::from)
        .collect()
}

fn create_group(name: &str) {
    let output = Command::new("groupadd")
        .arg(name)
        .output()
        .expect("groupadd");
    assert!(
        output.status.success(),
        "groupadd `{name}` failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}

fn delete_user(name: &str) {
    let output = Command::new("userdel").arg(name).output().expect("userdel");
    assert!(
        output.status.success(),
        "userdel `{name}` failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}

#[tokio::test]
#[ignore = "requires container with root; run via cargo test -- --include-ignored"]
async fn create_user_that_does_not_exist() {
    assert_in_container();
    let env = Env::synthetic(false);

    let user = User {
        name: "e2e_create".into(),
        uid: Some(20001),
        comment: Some("e2e create test".into()),
        home: Some(PathBuf::from("/home/e2e_create")),
        shell: Some(PathBuf::from("/bin/bash")),
        create_home: true,
        ..Default::default()
    };

    assert!(
        !user_exists("e2e_create"),
        "test precondition: e2e_create must not exist yet",
    );

    let result = user
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("converge: {e}"));
    assert_eq!(result, Changed::Yes);
    assert!(user_exists("e2e_create"), "user not created");

    let entry = String::from_utf8(
        Command::new("getent")
            .args(["passwd", "e2e_create"])
            .output()
            .expect("getent")
            .stdout,
    )
    .expect("utf8");
    assert!(
        entry.contains(":20001:"),
        "uid not 20001; getent line: {entry}",
    );
    assert!(
        entry.contains("/home/e2e_create"),
        "home wrong; getent line: {entry}",
    );
    assert!(
        entry.contains("/bin/bash"),
        "shell wrong; getent line: {entry}",
    );
}

#[tokio::test]
#[ignore = "requires container with root; run via cargo test -- --include-ignored"]
async fn re_converge_existing_user_is_unchanged() {
    assert_in_container();
    let env = Env::synthetic(false);

    let user = User {
        name: "e2e_idempotent".into(),
        uid: Some(20002),
        home: Some(PathBuf::from("/home/e2e_idempotent")),
        shell: Some(PathBuf::from("/bin/bash")),
        create_home: true,
        ..Default::default()
    };

    let first = user
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("first converge: {e}"));
    assert_eq!(first, Changed::Yes);

    let second = user
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("second converge: {e}"));
    assert_eq!(
        second,
        Changed::No,
        "second converge of unchanged user must report Changed::No",
    );
}

#[tokio::test]
#[ignore = "requires container with root; run via cargo test -- --include-ignored"]
async fn add_user_to_new_supplementary_group() {
    assert_in_container();
    let env = Env::synthetic(false);
    create_group("e2e_groupadd_target");

    let user = User {
        name: "e2e_groupadd".into(),
        uid: Some(20003),
        home: Some(PathBuf::from("/home/e2e_groupadd")),
        shell: Some(PathBuf::from("/bin/bash")),
        create_home: true,
        ..Default::default()
    };
    user.converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("create: {e}"));
    assert!(
        !supplementary_groups("e2e_groupadd").contains(&"e2e_groupadd_target".to_string()),
        "test precondition: user must not yet be in target group",
    );

    let with_group = User {
        groups: vec!["e2e_groupadd_target".to_string()],
        ..user
    };
    let result = with_group
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("add group: {e}"));
    assert_eq!(result, Changed::Yes);
    assert!(
        supplementary_groups("e2e_groupadd").contains(&"e2e_groupadd_target".to_string()),
        "user not in target group after converge",
    );
}

#[tokio::test]
#[ignore = "requires container with root; run via cargo test -- --include-ignored"]
async fn missing_group_fails_loudly() {
    assert_in_container();
    let env = Env::synthetic(false);

    let user = User {
        name: "e2e_missinggrp".into(),
        uid: Some(20004),
        home: Some(PathBuf::from("/home/e2e_missinggrp")),
        shell: Some(PathBuf::from("/bin/bash")),
        groups: vec!["definitely_not_a_real_group_e2e".to_string()],
        create_home: true,
        ..Default::default()
    };
    assert!(
        !group_exists("definitely_not_a_real_group_e2e"),
        "test precondition: synthetic group must not exist",
    );

    let err = user
        .converge_one(&env)
        .await
        .expect_err("creating a user with a non-existent group must fail loudly");
    let msg = err.to_string();
    assert!(
        msg.contains("definitely_not_a_real_group_e2e") || msg.contains("does not exist"),
        "error should mention the missing group; got: {msg}",
    );
    assert!(
        !user_exists("e2e_missinggrp"),
        "user must not have been created when group ref was bad",
    );
}

#[tokio::test]
#[ignore = "requires container with root; run via cargo test -- --include-ignored"]
async fn dry_run_does_not_create_user() {
    assert_in_container();
    let env = Env::synthetic(false).with_run_mode(host_setup::env::RunMode::DryRun);

    let user = User {
        name: "e2e_dryrun".into(),
        uid: Some(20005),
        home: Some(PathBuf::from("/home/e2e_dryrun")),
        shell: Some(PathBuf::from("/bin/bash")),
        create_home: true,
        ..Default::default()
    };

    let result = user
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("dry-run converge: {e}"));
    assert_eq!(result, Changed::Yes);
    assert!(
        !user_exists("e2e_dryrun"),
        "dry-run must not have created the user",
    );
}

#[tokio::test]
#[ignore = "requires container with root; run via cargo test -- --include-ignored"]
async fn password_is_set_on_creation() {
    assert_in_container();
    let env = Env::synthetic(false);

    // SHA-512 crypt of an arbitrary throwaway string. Only the format matters
    // here; we never log in.
    let hash = "$6$saltsalt$6JYpf3wMRlplbS59KRsRQ4WgyfPgK86CbN3pnG7zxDLK0t8WBe1qUk6r5pfrlVmrfVE.RuhV0DtQ4uFPNpmzm/";

    let user = User {
        name: "e2e_pwd".into(),
        uid: Some(20006),
        home: Some(PathBuf::from("/home/e2e_pwd")),
        shell: Some(PathBuf::from("/bin/bash")),
        password_hash: Some(hash.to_string()),
        create_home: true,
        ..Default::default()
    };
    user.converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("create: {e}"));

    let shadow = std::fs::read_to_string("/etc/shadow").expect("read /etc/shadow");
    let line = shadow
        .lines()
        .find(|l| l.starts_with("e2e_pwd:"))
        .unwrap_or_else(|| panic!("e2e_pwd line missing from /etc/shadow"));
    assert!(
        line.contains(hash),
        "password hash not set in /etc/shadow; line: {line}",
    );
}

#[tokio::test]
#[ignore = "requires container with root; run via cargo test -- --include-ignored"]
async fn recreate_after_external_delete_works() {
    assert_in_container();
    let env = Env::synthetic(false);

    let user = User {
        name: "e2e_recreate".into(),
        uid: Some(20007),
        home: Some(PathBuf::from("/home/e2e_recreate")),
        shell: Some(PathBuf::from("/bin/bash")),
        create_home: true,
        ..Default::default()
    };

    user.converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("first create: {e}"));
    assert!(user_exists("e2e_recreate"));

    delete_user("e2e_recreate");
    assert!(!user_exists("e2e_recreate"));

    let result = user
        .converge_one(&env)
        .await
        .unwrap_or_else(|e| panic!("re-create: {e}"));
    assert_eq!(
        result,
        Changed::Yes,
        "re-converge after external delete must re-create",
    );
    assert!(user_exists("e2e_recreate"));
}
