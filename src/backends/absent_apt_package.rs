use std::sync::Arc;

use async_trait::async_trait;
use tracing::{debug, info};

use crate::batcher::Batcher;
use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{BatchFamily, Changed, Resource, ResourceId, Skip};

use super::apt_package::{read_installed_packages, run_apt_remove};

const BACKEND: &str = "absent-apt-package";

/// Ensure an apt package is removed (purged). Companion to
/// [`AptPackage`](super::apt_package::AptPackage).
#[derive(Debug, Default)]
pub struct AbsentAptPackage {
    pub name: String,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for AbsentAptPackage {
    fn id_hint(&self) -> String {
        format!("absent-apt-package:{}", self.name)
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    fn batch_family(&self) -> Option<BatchFamily> {
        Some(BatchFamily::AbsentAptPackage)
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let installed = read_installed_packages().await?;
        if !installed.contains(&self.name) {
            return Ok(Changed::No);
        }
        if env.is_dry_run() {
            debug!(package = %self.name, "dry-run: would apt-get remove");
            return Ok(Changed::Yes);
        }
        run_apt_remove(std::slice::from_ref(&self.name)).await?;
        Ok(Changed::Yes)
    }
}

#[derive(Debug)]
pub struct AbsentAptPackageBatcher;

#[async_trait]
impl Batcher for AbsentAptPackageBatcher {
    fn family_name(&self) -> &'static str {
        BatchFamily::AbsentAptPackage.name()
    }

    async fn converge_batch(
        &self,
        resources: &[Arc<dyn Resource>],
        env: &Env,
    ) -> Result<Vec<Changed>, BackendError> {
        let pkgs: Vec<&AbsentAptPackage> = resources
            .iter()
            .map(|r| {
                r.as_any()
                    .downcast_ref::<AbsentAptPackage>()
                    .ok_or_else(|| {
                        BackendError::new(
                            BACKEND,
                            format!(
                                "batcher received non-AbsentAptPackage resource `{}`",
                                r.id_hint()
                            ),
                        )
                    })
            })
            .collect::<Result<_, _>>()?;

        let installed = read_installed_packages().await?;
        let to_remove: Vec<String> = pkgs
            .iter()
            .filter(|p| installed.contains(&p.name))
            .map(|p| p.name.clone())
            .collect();

        if to_remove.is_empty() {
            debug!("no batched apt packages to remove");
        } else if env.is_dry_run() {
            info!(count = to_remove.len(), packages = ?to_remove, "dry-run: would apt-get remove batched apt packages");
        } else {
            info!(count = to_remove.len(), packages = ?to_remove, "removing batched apt packages");
            run_apt_remove(&to_remove).await?;
        }

        Ok(pkgs
            .iter()
            .map(|p| {
                if installed.contains(&p.name) {
                    Changed::Yes
                } else {
                    Changed::No
                }
            })
            .collect())
    }
}
