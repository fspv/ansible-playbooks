use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;
use tracing::debug;

use crate::backends::file::File;
use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "systemd-unit";

/// A systemd unit file installed under `/etc/systemd/system/<name>`.
///
/// `name` includes the suffix (e.g. `node_exporter.service`,
/// `docker-cleanup.timer`, `home-user-.cache.mount`). When the file's
/// content drifts, this backend writes the new content atomically and runs
/// `systemctl daemon-reload` so any downstream `Service` resource sees
/// the updated unit. `daemon-reload` is encapsulated here — bundle authors
/// don't have to wire it themselves.
#[derive(Debug, Default)]
pub struct SystemdUnit {
    pub name: String,
    pub content: String,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for SystemdUnit {
    fn id_hint(&self) -> String {
        format!("systemd-unit:{}", self.name)
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let file = File {
            path: PathBuf::from(format!("/etc/systemd/system/{}", self.name)),
            content: self.content.clone(),
            mode: Some(Permissions::from_mode(0o644)),
            ..Default::default()
        };
        let changed = file.converge_one(env).await?;
        if changed == Changed::Yes && !env.is_dry_run() {
            debug!(unit = %self.name, "running systemctl daemon-reload after unit write");
            run_daemon_reload().await?;
        }
        Ok(changed)
    }
}

async fn run_daemon_reload() -> Result<(), BackendError> {
    let output = Command::new("systemctl")
        .arg("daemon-reload")
        .output()
        .await
        .map_err(|e| BackendError::with_source(BACKEND, "spawn `systemctl daemon-reload`", e))?;
    if !output.status.success() {
        return Err(BackendError::new(
            BACKEND,
            format!(
                "systemctl daemon-reload failed: {}",
                String::from_utf8_lossy(&output.stderr).trim(),
            ),
        ));
    }
    Ok(())
}
