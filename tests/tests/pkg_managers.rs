use std::time::Duration;

use ansible_playbook_tests::{run_command_must_succeed, run_command_must_succeed_within};

const NETWORK_TIMEOUT: Duration = Duration::from_secs(30);

#[test]
fn snap_list_works_for_user() {
    let out = run_command_must_succeed("snap", &["list"]).unwrap_or_else(|e| panic!("{e}"));
    let body = String::from_utf8_lossy(&out.stdout);
    assert!(
        body.lines().any(|l| l.starts_with("Name ")) || body.contains("snapd"),
        "`snap list` produced no recognisable output:\n{body}"
    );
}

#[test]
fn flatpak_remotes_includes_flathub() {
    let out = run_command_must_succeed("flatpak", &["remotes", "--columns=name"])
        .unwrap_or_else(|e| panic!("{e}"));
    let body = String::from_utf8_lossy(&out.stdout);
    assert!(
        body.lines().any(|l| l.trim() == "flathub"),
        "flathub remote not registered:\n{body}"
    );
}

#[test]
fn nix_store_ping_succeeds() {
    run_command_must_succeed_within("nix", &["store", "ping"], NETWORK_TIMEOUT)
        .unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn python3_venv_is_usable() {
    let dir = std::env::temp_dir().join(format!("ansible-tests-venv-{}", std::process::id()));
    let dir_str = dir.to_string_lossy().into_owned();
    let _ = std::fs::remove_dir_all(&dir);

    run_command_must_succeed("python3", &["-m", "venv", &dir_str])
        .unwrap_or_else(|e| panic!("{e}"));
    let pip = dir.join("bin/pip");
    let pip_str = pip.to_string_lossy().into_owned();
    run_command_must_succeed(&pip_str, &["--version"]).unwrap_or_else(|e| panic!("{e}"));

    let _ = std::fs::remove_dir_all(&dir);
}
