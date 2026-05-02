//! Functional rootless-podman tests.
//!
//! These run as the invoking user (no sudo) and exercise the rootless
//! configuration the `docker` role lays down — subuid/subgid, storage,
//! networking — by actually running a container.

use std::time::Duration;

use ansible_playbook_tests::{run_ok, run_ok_timeout};

const HELLO_WORLD: &str = "docker.io/library/hello-world";
const PULL_TIMEOUT: Duration = Duration::from_mins(2);

#[test]
fn podman_info_reports_rootless() {
    let out = run_ok("podman", &["info"]).unwrap_or_else(|e| panic!("{e}"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("rootless"),
        "`podman info` is missing the `rootless` marker:\n{stdout}"
    );
}

#[test]
fn podman_run_hello_world() {
    let out = run_ok_timeout("podman", &["run", "--rm", HELLO_WORLD], PULL_TIMEOUT)
        .unwrap_or_else(|e| panic!("{e}"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Hello from Docker!"),
        "unexpected hello-world output:\n{stdout}"
    );
}
