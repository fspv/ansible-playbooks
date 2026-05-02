use ansible_playbook_tests::{
    assert_systemd_unit_is_active, assert_systemd_unit_is_enabled, run_command_must_succeed,
};

#[test]
fn docker_service_enabled() {
    assert_systemd_unit_is_enabled("docker.service");
}

#[test]
fn docker_service_active() {
    assert_systemd_unit_is_active("docker.service");
}

#[test]
fn docker_cleanup_timer_enabled() {
    assert_systemd_unit_is_enabled("docker-cleanup.timer");
}

#[test]
fn docker_cleanup_timer_active() {
    assert_systemd_unit_is_active("docker-cleanup.timer");
}

#[test]
fn podman_cleanup_timer_enabled() {
    assert_systemd_unit_is_enabled("podman-cleanup.timer");
}

#[test]
fn podman_cleanup_timer_active() {
    assert_systemd_unit_is_active("podman-cleanup.timer");
}

#[test]
fn cleanup_timers_listed() {
    let out = run_command_must_succeed("systemctl", &["list-timers", "--all", "--no-pager"])
        .unwrap_or_else(|e| panic!("{e}"));
    let stdout = String::from_utf8_lossy(&out.stdout);
    for unit in ["docker-cleanup.timer", "podman-cleanup.timer"] {
        assert!(
            stdout.contains(unit),
            "{unit} missing from list-timers:\n{stdout}"
        );
    }
}

#[test]
fn et_service_enabled() {
    assert_systemd_unit_is_enabled("et.service");
}

#[test]
fn et_service_active() {
    assert_systemd_unit_is_active("et.service");
}

#[test]
fn apparmor_service_enabled() {
    assert_systemd_unit_is_enabled("apparmor.service");
}

#[test]
fn apparmor_service_active() {
    assert_systemd_unit_is_active("apparmor.service");
}

#[test]
fn networkmanager_service_enabled() {
    assert_systemd_unit_is_enabled("NetworkManager.service");
}

#[test]
fn networkmanager_service_active() {
    assert_systemd_unit_is_active("NetworkManager.service");
}
