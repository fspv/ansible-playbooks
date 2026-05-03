use async_trait::async_trait;
use tokio::process::Command;
use tracing::debug;

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "service";

/// Runtime state of a systemd unit (service, timer, mount, …).
///
/// Only "ensure enabled" and "ensure started" are supported — the backend
/// never disables or stops a unit. Both flags default to `false` meaning
/// "leave alone"; set them to `true` to ensure that state. `name` includes
/// the suffix (e.g. `docker.service`, `docker-cleanup.timer`).
///
/// `restart_on` is the ansible-`notify` equivalent: list any `ResourceId`
/// whose change should trigger `systemctl restart <name>` even when the
/// unit is already enabled and active. Typical use is the unit's config
/// files / drop-ins. When `started` is true and any `restart_on` id
/// changed, this resource emits one `systemctl restart` (which also covers
/// the "not running yet" case).
#[derive(Debug, Default)]
pub struct Service {
    pub name: String,
    pub enabled: bool,
    pub started: bool,
    pub restart_on: Vec<ResourceId>,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for Service {
    fn id_hint(&self) -> String {
        format!("service:{}", self.name)
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let is_enabled = systemctl_status("is-enabled", &self.name).await?;
        let is_active = systemctl_status("is-active", &self.name).await?;

        let needs_enable = self.enabled && !is_enabled;
        let needs_start = self.started && !is_active;
        let needs_restart = self.started && is_active && env.any_changed(&self.restart_on).await;

        if !needs_enable && !needs_start && !needs_restart {
            return Ok(Changed::No);
        }

        if env.is_dry_run() {
            debug!(
                service = %self.name,
                will_enable = needs_enable,
                will_start = needs_start,
                will_restart = needs_restart,
                "dry-run: would adjust service",
            );
            return Ok(Changed::Yes);
        }

        if needs_enable {
            run_systemctl(&["enable", &self.name]).await?;
        }
        if needs_restart {
            run_systemctl(&["restart", &self.name]).await?;
        } else if needs_start {
            run_systemctl(&["start", &self.name]).await?;
        }
        Ok(Changed::Yes)
    }
}

async fn systemctl_status(verb: &str, name: &str) -> Result<bool, BackendError> {
    // `systemctl is-enabled <name>` exits 0 for enabled / static / linked /
    // alias / generated / transient — anything systemd considers "active
    // configuration" — and non-zero for disabled / masked / not-found.
    // `systemctl is-active <name>` exits 0 for active / activating, non-zero
    // otherwise. For our "ensure ... " semantics, exit code is enough.
    let output = Command::new("systemctl")
        .arg(verb)
        .arg(name)
        .output()
        .await
        .map_err(|e| {
            BackendError::with_source(BACKEND, format!("spawn `systemctl {verb} {name}`"), e)
        })?;
    Ok(output.status.success())
}

async fn run_systemctl(args: &[&str]) -> Result<(), BackendError> {
    let output = Command::new("systemctl")
        .args(args)
        .output()
        .await
        .map_err(|e| {
            BackendError::with_source(BACKEND, format!("spawn `systemctl {}`", args.join(" ")), e)
        })?;
    if !output.status.success() {
        return Err(BackendError::new(
            BACKEND,
            format!(
                "`systemctl {}` failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim(),
            ),
        ));
    }
    Ok(())
}
