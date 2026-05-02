use std::collections::HashSet;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::process::Command;
use tracing::{debug, info};

use crate::batcher::Batcher;
use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{BatchFamily, Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "apt-package";

#[derive(Debug, Default)]
pub struct AptPackage {
    pub name: String,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for AptPackage {
    fn id_hint(&self) -> String {
        format!("apt-package:{}", self.name)
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    fn batch_family(&self) -> Option<BatchFamily> {
        Some(BatchFamily::AptPackage)
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let installed = read_installed_packages().await?;
        if installed.contains(&self.name) {
            return Ok(Changed::No);
        }
        if env.is_dry_run() {
            debug!(package = %self.name, "dry-run: would apt-get update + install");
            return Ok(Changed::Yes);
        }
        run_apt_update().await?;
        run_apt_install(std::slice::from_ref(&self.name)).await?;
        Ok(Changed::Yes)
    }
}

#[derive(Debug)]
pub struct AptPackageBatcher;

#[async_trait]
impl Batcher for AptPackageBatcher {
    fn family_name(&self) -> &'static str {
        BatchFamily::AptPackage.name()
    }

    async fn converge_batch(
        &self,
        resources: &[Arc<dyn Resource>],
        env: &Env,
    ) -> Result<Vec<Changed>, BackendError> {
        let pkgs: Vec<&AptPackage> = resources
            .iter()
            .map(|r| {
                r.as_any().downcast_ref::<AptPackage>().ok_or_else(|| {
                    BackendError::new(
                        BACKEND,
                        format!("batcher received non-AptPackage resource `{}`", r.id_hint()),
                    )
                })
            })
            .collect::<Result<_, _>>()?;

        let installed = read_installed_packages().await?;
        let to_install: Vec<String> = pkgs
            .iter()
            .filter(|p| !installed.contains(&p.name))
            .map(|p| p.name.clone())
            .collect();

        if to_install.is_empty() {
            debug!("all batched apt packages already installed");
        } else if env.is_dry_run() {
            info!(count = to_install.len(), packages = ?to_install, "dry-run: would apt-get update + install batched apt packages");
        } else {
            info!(count = to_install.len(), packages = ?to_install, "running apt-get update then installing batched apt packages");
            run_apt_update().await?;
            run_apt_install(&to_install).await?;
        }

        Ok(pkgs
            .iter()
            .map(|p| {
                if installed.contains(&p.name) {
                    Changed::No
                } else {
                    Changed::Yes
                }
            })
            .collect())
    }
}

async fn read_installed_packages() -> Result<HashSet<String>, BackendError> {
    let output = Command::new("dpkg-query")
        .args(["-W", "-f=${Package}\t${db:Status-Abbrev}\n"])
        .output()
        .await
        .map_err(|e| BackendError::with_source(BACKEND, "spawn dpkg-query to read apt state", e))?;

    if !output.status.success() {
        return Err(BackendError::new(
            BACKEND,
            format!(
                "dpkg-query exited with {:?}; stderr:\n{}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr),
            ),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut installed = HashSet::new();
    for line in stdout.lines() {
        let mut parts = line.split('\t');
        let (Some(name), Some(status)) = (parts.next(), parts.next()) else {
            continue;
        };
        if status.starts_with("ii") {
            installed.insert(name.to_string());
        }
    }
    Ok(installed)
}

async fn run_apt_update() -> Result<(), BackendError> {
    let output = Command::new("apt-get")
        .env("DEBIAN_FRONTEND", "noninteractive")
        .arg("update")
        .output()
        .await
        .map_err(|e| BackendError::with_source(BACKEND, "spawn apt-get update", e))?;

    if !output.status.success() {
        return Err(BackendError::new(
            BACKEND,
            format!(
                "apt-get update failed; stderr:\n{}",
                String::from_utf8_lossy(&output.stderr),
            ),
        ));
    }
    Ok(())
}

async fn run_apt_install(names: &[String]) -> Result<(), BackendError> {
    if names.is_empty() {
        return Ok(());
    }
    let output = Command::new("apt-get")
        .env("DEBIAN_FRONTEND", "noninteractive")
        .arg("install")
        .arg("-y")
        .args(names)
        .output()
        .await
        .map_err(|e| BackendError::with_source(BACKEND, "spawn apt-get install", e))?;

    if !output.status.success() {
        return Err(BackendError::new(
            BACKEND,
            format!(
                "apt-get install failed; stderr:\n{}",
                String::from_utf8_lossy(&output.stderr),
            ),
        ));
    }
    Ok(())
}
