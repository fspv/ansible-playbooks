//! Membership in `docker` or `libvirt` silently grants root-equivalent access
//! — these negative tests guard against a role accidentally adding the user.

use std::io;
use std::os::unix::net::UnixStream;
use std::path::Path;

use ansible_playbook_tests::{assert_current_user_not_in_group, run_command_must_fail};

#[test]
fn user_not_in_docker_group() {
    assert_current_user_not_in_group("docker");
}

#[test]
fn docker_daemon_socket_inaccessible() {
    let socket = Path::new("/var/run/docker.sock");
    assert!(
        socket.exists(),
        "/var/run/docker.sock missing — docker daemon expected to be running"
    );
    match UnixStream::connect(socket) {
        Ok(_) => panic!("connected to /var/run/docker.sock as unprivileged user"),
        Err(e) => assert_eq!(
            e.kind(),
            io::ErrorKind::PermissionDenied,
            "wanted PermissionDenied, got {:?}: {e}",
            e.kind()
        ),
    }
}

#[test]
fn user_not_in_libvirt_group() {
    assert_current_user_not_in_group("libvirt");
}

#[test]
fn user_not_in_libvirtd_group() {
    assert_current_user_not_in_group("libvirtd");
}

#[test]
fn virsh_system_uri_requires_privilege() {
    let out = run_command_must_fail("virsh", &["-c", "qemu:///system", "list"])
        .unwrap_or_else(|e| panic!("{e}"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("authentication")
            || stderr.contains("permission")
            || stderr.contains("denied"),
        "expected an auth/permission error from `virsh -c qemu:///system list`, got:\n{stderr}"
    );
}
