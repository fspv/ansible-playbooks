use ansible_playbook_tests::{assert_program_is_on_path, run_command_must_succeed};

fn assert_runs(program: &str, args: &[&str]) {
    assert_program_is_on_path(program);
    run_command_must_succeed(program, args).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn docker_present() {
    assert_runs("docker", &["--version"]);
}

#[test]
fn docker_compose_plugin_present() {
    assert_runs("docker", &["compose", "version"]);
}

#[test]
fn docker_buildx_plugin_present() {
    assert_runs("docker", &["buildx", "version"]);
}

#[test]
fn podman_present() {
    assert_runs("podman", &["--version"]);
}

#[test]
fn podman_compose_present() {
    assert_runs("podman-compose", &["--version"]);
}

#[test]
fn crun_present() {
    assert_runs("crun", &["--version"]);
}

#[test]
fn nvim_present() {
    assert_runs("nvim", &["--version"]);
}

#[test]
fn et_present() {
    assert_runs("et", &["--help"]);
}

#[test]
fn python3_present() {
    assert_runs("python3", &["--version"]);
}

#[test]
fn pip3_present() {
    assert_runs("pip3", &["--version"]);
}

#[test]
fn virtualenv_present() {
    assert_runs("virtualenv", &["--version"]);
}

#[test]
fn snap_present() {
    assert_runs("snap", &["--version"]);
}

#[test]
fn flatpak_present() {
    assert_runs("flatpak", &["--version"]);
}

#[test]
fn nix_present() {
    assert_runs("nix", &["--version"]);
}

#[test]
fn ykman_present() {
    assert_runs("ykman", &["--version"]);
}

#[test]
fn pamu2fcfg_present() {
    assert_runs("pamu2fcfg", &["--help"]);
}

#[test]
fn qemu_system_x86_64_present() {
    assert_runs("qemu-system-x86_64", &["--version"]);
}

#[test]
fn qemu_system_aarch64_present() {
    assert_runs("qemu-system-aarch64", &["--version"]);
}

#[test]
fn qemu_img_present() {
    assert_runs("qemu-img", &["--version"]);
}

#[test]
fn virsh_present() {
    assert_runs("virsh", &["--version"]);
}

#[test]
fn uvt_kvm_present() {
    // `uvt-kvm --help` exits non-zero on Ubuntu when distro-info-data is
    // stale
    assert_program_is_on_path("uvt-kvm");
}

#[test]
fn timedatectl_present() {
    assert_runs("timedatectl", &["--version"]);
}

#[test]
fn gpg_present() {
    assert_runs("gpg", &["--version"]);
}

#[test]
fn locale_present() {
    assert_runs("locale", &["--version"]);
}

#[test]
fn systemctl_present() {
    assert_runs("systemctl", &["--version"]);
}

#[test]
fn apparmor_parser_present() {
    assert_runs("apparmor_parser", &["--version"]);
}
