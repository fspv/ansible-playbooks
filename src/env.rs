use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunMode {
    #[default]
    Apply,
    DryRun,
}

#[derive(Debug, Clone)]
pub struct Env {
    is_container: bool,
    run_mode: RunMode,
    architecture: String,
}

impl Env {
    #[must_use]
    pub fn detect() -> Self {
        Self {
            is_container: detect_container(),
            run_mode: RunMode::default(),
            architecture: detect_architecture(),
        }
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
        }
    }

    #[must_use]
    pub const fn with_run_mode(mut self, mode: RunMode) -> Self {
        self.run_mode = mode;
        self
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
