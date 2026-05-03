use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::error::Error;
use crate::resource::ResourceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunMode {
    #[default]
    Apply,
    DryRun,
}

#[derive(Debug, Default)]
struct RunState {
    changed: RwLock<HashSet<ResourceId>>,
}

#[derive(Debug, Clone)]
pub struct Env {
    is_container: bool,
    run_mode: RunMode,
    architecture: String,
    kernel_release: String,
    ubuntu_codename: String,
    is_in_virtualbox: bool,
    run_state: Arc<RunState>,
}

impl Env {
    /// Detect host facts (container? arch? codename? `VirtualBox` guest?).
    /// Called once at startup.
    ///
    /// # Errors
    /// Returns [`Error::EnvDetect`] when a required probe cannot complete.
    /// Currently the only fallible probe is reading `/etc/os-release` for
    /// the Ubuntu codename — every supported host has it, so a missing or
    /// unparseable file means we'd be guessing and the framework would bake
    /// the wrong codename into apt repo URLs.
    pub fn detect() -> Result<Self, Error> {
        Ok(Self {
            is_container: detect_container(),
            run_mode: RunMode::default(),
            architecture: detect_architecture(),
            kernel_release: detect_kernel_release(),
            ubuntu_codename: detect_ubuntu_codename()?,
            is_in_virtualbox: detect_in_virtualbox(),
            run_state: Arc::default(),
        })
    }

    #[must_use]
    pub fn synthetic(is_container: bool) -> Self {
        Self::synthetic_with_architecture(is_container, "x86_64")
    }

    #[must_use]
    pub fn synthetic_with_architecture(is_container: bool, architecture: &str) -> Self {
        Self {
            is_container,
            run_mode: RunMode::Apply,
            architecture: architecture.to_string(),
            kernel_release: detect_kernel_release(),
            // Synthetic envs don't probe /etc/os-release. "noble" matches the
            // host the framework was originally written against; tests that
            // care about codename should set it explicitly via a builder.
            ubuntu_codename: "noble".to_string(),
            is_in_virtualbox: false,
            run_state: Arc::default(),
        }
    }

    #[must_use]
    pub const fn with_run_mode(mut self, mode: RunMode) -> Self {
        self.run_mode = mode;
        self
    }

    /// Record that `id` produced a change in the current run. Called by the
    /// executor after a level completes so resources in subsequent levels
    /// can react via [`Self::any_changed`].
    pub async fn record_changed(&self, id: ResourceId) {
        self.run_state.changed.write().await.insert(id);
    }

    /// True if any of `ids` has been recorded as changed in the current run.
    /// Used by `Service` and `Command` to gate restart / trigger semantics.
    pub async fn any_changed(&self, ids: &[ResourceId]) -> bool {
        let lock = self.run_state.changed.read().await;
        ids.iter().any(|id| lock.contains(id))
    }

    #[must_use]
    pub const fn run_mode(&self) -> RunMode {
        self.run_mode
    }

    #[must_use]
    pub const fn is_dry_run(&self) -> bool {
        matches!(self.run_mode, RunMode::DryRun)
    }

    #[must_use]
    pub const fn is_container(&self) -> bool {
        self.is_container
    }

    #[must_use]
    pub const fn is_real_machine(&self) -> bool {
        !self.is_container
    }

    #[must_use]
    pub fn architecture(&self) -> &str {
        &self.architecture
    }

    #[must_use]
    pub fn kernel_release(&self) -> &str {
        &self.kernel_release
    }

    /// Map host architecture (`uname -m` output) to the apt arch string used
    /// in `[arch=...]` repo lines.
    #[must_use]
    pub fn apt_arch(&self) -> &'static str {
        match self.architecture.as_str() {
            "armv7l" | "aarch64" => "arm64",
            _ => "amd64",
        }
    }

    /// True for the 32/64-bit ARM architectures Ubuntu serves from
    /// `ports.ubuntu.com` rather than the main archive.
    #[must_use]
    pub fn is_arm(&self) -> bool {
        matches!(self.architecture.as_str(), "armv7l" | "aarch64")
    }

    /// Ubuntu release codename (e.g. `noble`, `jammy`) read from
    /// `/etc/os-release`. Used by bundles that bake the codename into apt
    /// repository URLs and pin files. Falls back to `noble` if the file is
    /// missing or unparseable, matching the framework's primary target.
    #[must_use]
    pub fn ubuntu_codename(&self) -> &str {
        &self.ubuntu_codename
    }

    /// True when the host is running inside a `VirtualBox` VM. Detected
    /// once at startup via `lspci`. Used by the virtualbox bundle to decide
    /// whether guest additions should be installed.
    #[must_use]
    pub const fn is_in_virtualbox(&self) -> bool {
        self.is_in_virtualbox
    }
}

fn detect_container() -> bool {
    if Path::new("/.dockerenv").exists() {
        return true;
    }
    if Path::new("/run/.containerenv").exists() {
        return true;
    }
    fs::read_to_string("/proc/1/cgroup")
        .is_ok_and(|cg| cg.contains("/docker/") || cg.contains("/lxc/") || cg.contains("kubepods"))
}

fn detect_architecture() -> String {
    match Command::new("uname").arg("-m").output() {
        Ok(out) if out.status.success() => {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() {
                std::env::consts::ARCH.to_string()
            } else {
                s
            }
        }
        _ => std::env::consts::ARCH.to_string(),
    }
}

fn detect_kernel_release() -> String {
    match Command::new("uname").arg("-r").output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => String::new(),
    }
}

fn detect_ubuntu_codename() -> Result<String, Error> {
    let path = "/etc/os-release";
    let raw = fs::read_to_string(path).map_err(|e| Error::EnvDetect {
        what: "read /etc/os-release",
        source: Box::new(e),
    })?;
    // Prefer VERSION_CODENAME; fall back to UBUNTU_CODENAME (older releases).
    for key in ["VERSION_CODENAME", "UBUNTU_CODENAME"] {
        for line in raw.lines() {
            let Some(rest) = line.strip_prefix(key) else {
                continue;
            };
            let Some(val) = rest.strip_prefix('=') else {
                continue;
            };
            let trimmed = val.trim().trim_matches('"').trim_matches('\'');
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }
    Err(Error::EnvDetect {
        what: "ubuntu codename",
        source: format!("neither VERSION_CODENAME nor UBUNTU_CODENAME found in {path}").into(),
    })
}

fn detect_in_virtualbox() -> bool {
    // `lspci` is the same probe roles/virtualbox/tasks/packages.yml uses.
    // Missing binary or non-zero exit means "not VirtualBox".
    Command::new("lspci")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .is_some_and(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .any(|l| l.contains("VirtualBox"))
        })
}
