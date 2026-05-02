use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use ansible_playbook_tests::{current_user_home_dir, read_file_to_string, read_symlink_target};

#[test]
fn docker_daemon_json_present() {
    let body =
        read_file_to_string(Path::new("/etc/docker/daemon.json")).unwrap_or_else(|e| panic!("{e}"));
    for needle in [
        "\"iptables\"",
        "\"ipv6\"",
        "\"fixed-cidr-v6\"",
        "\"metrics-addr\"",
    ] {
        assert!(
            body.contains(needle),
            "{needle} missing in daemon.json:\n{body}"
        );
    }
}

#[test]
fn docker_daemon_json_metrics_port() {
    let body =
        read_file_to_string(Path::new("/etc/docker/daemon.json")).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        body.contains("0.0.0.0:9323"),
        "metrics-addr 0.0.0.0:9323 missing in daemon.json:\n{body}"
    );
}

#[test]
fn docker_user_run_args_present_and_locked_down() {
    let p = current_user_home_dir().join(".config/docker-user/run-args");
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    assert!(meta.is_file(), "{} is not a regular file", p.display());
    let mode = meta.permissions().mode() & 0o777;
    assert_eq!(
        mode,
        0o600,
        "{} mode is 0o{mode:o}, want 0o600",
        p.display()
    );
}

#[test]
fn yubico_udev_rule_installed() {
    let body = read_file_to_string(Path::new("/etc/udev/rules.d/70-u2f.rules"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        body.contains("hidraw") || body.contains("u2f") || body.contains("Yubico"),
        "70-u2f.rules looks empty / wrong:\n{body}"
    );
}

#[test]
fn apparmor_bwrap_profile_installed() {
    let body =
        read_file_to_string(Path::new("/etc/apparmor.d/bwrap")).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        body.contains("profile bwrap"),
        "bwrap apparmor profile body looks wrong:\n{body}"
    );
}

#[test]
fn resolv_conf_is_regular_file_not_symlink() {
    let p = Path::new("/etc/resolv.conf");
    let meta =
        std::fs::symlink_metadata(p).unwrap_or_else(|e| panic!("lstat /etc/resolv.conf: {e}"));
    assert!(
        !meta.file_type().is_symlink(),
        "/etc/resolv.conf is still a symlink (resolv role should have replaced it); \
         points at {:?}",
        read_symlink_target(p).ok()
    );
}

#[test]
fn resolv_conf_has_nameserver() {
    let body = read_file_to_string(Path::new("/etc/resolv.conf")).unwrap_or_else(|e| panic!("{e}"));
    assert!(
        body.lines()
            .any(|l| l.trim_start().starts_with("nameserver ")),
        "no nameserver line in /etc/resolv.conf:\n{body}"
    );
}

#[test]
fn networkmanager_conf_disables_internal_dns() {
    let body = read_file_to_string(Path::new("/etc/NetworkManager/NetworkManager.conf"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        body.contains("dns=none"),
        "dns=none missing in NetworkManager.conf:\n{body}"
    );
}
