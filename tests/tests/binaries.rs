//! Smoke checks that every binary the `common-devserver.yml` playbook is
//! expected to install resolves on `$PATH` and runs.
//!
//! We assert on user-visible binaries rather than package names: how the
//! binary landed on disk is an implementation detail of the playbook.

use ansible_playbook_tests::{assert_on_path, run_ok};

fn assert_runs(program: &str, args: &[&str]) {
    assert_on_path(program);
    run_ok(program, args).unwrap_or_else(|e| panic!("{e}"));
}

// ---------- container runtimes ----------

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

// ---------- editor / terminal ----------

#[test]
fn nvim_present() {
    assert_runs("nvim", &["--version"]);
}

#[test]
fn et_present() {
    // `et --version` exits non-zero on some builds.
    assert_runs("et", &["--help"]);
}

// ---------- python ----------

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

// ---------- alt package systems ----------

#[test]
fn snap_present() {
    // `snap version` reaches snapd over a socket; `--version` is local-only.
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

// ---------- yubico ----------

#[test]
fn ykman_present() {
    assert_runs("ykman", &["--version"]);
}

#[test]
fn pamu2fcfg_present() {
    // pamu2fcfg has no `--version`.
    assert_runs("pamu2fcfg", &["--help"]);
}

// ---------- virtualization ----------

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
    // stale; presence on PATH is the contract under test.
    assert_on_path("uvt-kvm");
}

// ---------- common-tweaks tooling ----------

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
