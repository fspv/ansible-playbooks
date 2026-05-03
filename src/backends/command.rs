use async_trait::async_trait;
use tokio::process::Command as TokioCommand;
use tracing::debug;

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "command";

/// Run an external command.
///
/// By default reports `Changed::Yes` on every run — `Command` does not
/// sense state. Use this for inherently idempotent operations
/// (`apt-get update`, `systemctl daemon-reload`) or as a building block
/// depended on by resources that *do* sense state.
///
/// `trigger_on` is the ansible-`notify`-handler equivalent: when set,
/// the command only runs if at least one of the listed `ResourceId`s
/// changed in the current run. This is how `update-ca-certificates`,
/// `locale-gen`, `udevadm reload-rules`, etc. should be wired — they
/// are triggers, not unconditional steps.
#[derive(Debug, Default)]
pub struct Command {
    pub name: String,
    pub argv: Vec<String>,
    pub trigger_on: Option<Vec<ResourceId>>,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for Command {
    fn id_hint(&self) -> String {
        format!("command:{}", self.name)
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let Some((program, args)) = self.argv.split_first() else {
            return Err(BackendError::new(
                BACKEND,
                format!("command `{}` has empty argv", self.name),
            ));
        };

        if let Some(triggers) = self.trigger_on.as_deref() {
            if !env.any_changed(triggers).await {
                debug!(command = %self.name, "trigger_on set and no upstream change; skipping");
                return Ok(Changed::No);
            }
        }

        if env.is_dry_run() {
            debug!(command = %self.name, argv = ?self.argv, "dry-run: would run command");
            return Ok(Changed::Yes);
        }

        let output = TokioCommand::new(program)
            .args(args)
            .output()
            .await
            .map_err(|e| BackendError::with_source(BACKEND, format!("spawn `{}`", self.name), e))?;

        if !output.status.success() {
            return Err(BackendError::new(
                BACKEND,
                format!(
                    "command `{}` exited with {:?}; stderr:\n{}",
                    self.name,
                    output.status.code(),
                    String::from_utf8_lossy(&output.stderr),
                ),
            ));
        }

        Ok(Changed::Yes)
    }
}
