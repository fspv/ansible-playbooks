use std::path::PathBuf;

use async_trait::async_trait;
use tokio::process::Command;
use tracing::debug;

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "user";

#[derive(Debug, Default)]
pub struct User {
    pub name: String,
    pub uid: Option<u32>,
    pub comment: Option<String>,
    pub home: Option<PathBuf>,
    pub shell: Option<PathBuf>,
    pub groups: Vec<String>,
    pub password_hash: Option<String>,
    pub create_home: bool,
    pub system: bool,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for User {
    fn id_hint(&self) -> String {
        format!("user:{}", self.name)
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let exists = sense_user_exists(&self.name).await?;
        let current_groups = if exists {
            sense_supplementary_groups(&self.name).await?
        } else {
            Vec::new()
        };
        let groups_to_add: Vec<&str> = self
            .groups
            .iter()
            .filter(|g| !current_groups.contains(g))
            .map(String::as_str)
            .collect();

        if exists && groups_to_add.is_empty() {
            return Ok(Changed::No);
        }

        if env.is_dry_run() {
            debug!(
                user = %self.name,
                will_create = !exists,
                groups_to_add = ?groups_to_add,
                "dry-run: would adjust user",
            );
            return Ok(Changed::Yes);
        }

        if !exists {
            create_user(self).await?;
            if let Some(hash) = &self.password_hash {
                set_password(&self.name, hash).await?;
            }
        } else if !groups_to_add.is_empty() {
            add_to_groups(&self.name, &groups_to_add).await?;
        }

        Ok(Changed::Yes)
    }
}

async fn sense_user_exists(name: &str) -> Result<bool, BackendError> {
    let output = Command::new("getent")
        .args(["passwd", name])
        .output()
        .await
        .map_err(|e| {
            BackendError::with_source(BACKEND, format!("spawn `getent passwd {name}`"), e)
        })?;
    Ok(output.status.success())
}

async fn sense_supplementary_groups(name: &str) -> Result<Vec<String>, BackendError> {
    let output = Command::new("id")
        .args(["-nG", name])
        .output()
        .await
        .map_err(|e| BackendError::with_source(BACKEND, format!("spawn `id -nG {name}`"), e))?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(String::from)
        .collect())
}

async fn create_user(user: &User) -> Result<(), BackendError> {
    let mut cmd = Command::new("useradd");
    if let Some(uid) = user.uid {
        cmd.arg("-u").arg(uid.to_string());
    }
    if let Some(comment) = &user.comment {
        cmd.arg("-c").arg(comment);
    }
    if let Some(home) = &user.home {
        cmd.arg("-d").arg(home);
    }
    if let Some(shell) = &user.shell {
        cmd.arg("-s").arg(shell);
    }
    if !user.groups.is_empty() {
        cmd.arg("-G").arg(user.groups.join(","));
    }
    if user.create_home {
        cmd.arg("-m");
    }
    if user.system {
        cmd.arg("-r");
    }
    cmd.arg(&user.name);

    let output = cmd.output().await.map_err(|e| {
        BackendError::with_source(BACKEND, format!("spawn useradd for `{}`", user.name), e)
    })?;
    if !output.status.success() {
        return Err(BackendError::new(
            BACKEND,
            format!(
                "useradd `{}` failed: {}",
                user.name,
                String::from_utf8_lossy(&output.stderr).trim(),
            ),
        ));
    }
    Ok(())
}

async fn add_to_groups(name: &str, groups: &[&str]) -> Result<(), BackendError> {
    let output = Command::new("usermod")
        .arg("-aG")
        .arg(groups.join(","))
        .arg(name)
        .output()
        .await
        .map_err(|e| {
            BackendError::with_source(BACKEND, format!("spawn `usermod -aG ... {name}`"), e)
        })?;
    if !output.status.success() {
        return Err(BackendError::new(
            BACKEND,
            format!(
                "usermod -aG for `{name}` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim(),
            ),
        ));
    }
    Ok(())
}

async fn set_password(name: &str, hash: &str) -> Result<(), BackendError> {
    let output = Command::new("usermod")
        .arg("-p")
        .arg(hash)
        .arg(name)
        .output()
        .await
        .map_err(|e| {
            BackendError::with_source(BACKEND, format!("spawn `usermod -p ... {name}`"), e)
        })?;
    if !output.status.success() {
        return Err(BackendError::new(
            BACKEND,
            format!(
                "usermod -p for `{name}` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim(),
            ),
        ));
    }
    Ok(())
}
