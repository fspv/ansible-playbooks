use std::fs;
use std::path::Path;

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
}

impl Env {
    #[must_use]
    pub fn detect() -> Self {
        Self {
            is_container: detect_container(),
            run_mode: RunMode::default(),
        }
    }

    #[must_use]
    pub const fn synthetic(is_container: bool) -> Self {
        Self {
            is_container,
            run_mode: RunMode::Apply,
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
