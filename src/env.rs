use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use tokio::sync::RwLock;

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
    run_state: Arc<RunState>,
}

impl Env {
    #[must_use]
    pub fn detect() -> Self {
        Self {
            is_container: detect_container(),
            run_mode: RunMode::default(),
            architecture: detect_architecture(),
            run_state: Arc::default(),
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
