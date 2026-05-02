// uid/gid are unix terms-of-art that intentionally appear as a pair. Pedantic
// `similar_names` flags `want_uid`/`want_gid` as too close — they're meant to
// be parallel; renaming them harms readability more than it helps.
#![allow(clippy::similar_names)]

use std::fs::Permissions;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs;
use tokio::process::Command;
use tracing::debug;

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "directory";

#[derive(Debug, Default)]
pub struct Directory {
    pub path: PathBuf,
    pub mode: Option<Permissions>,
    pub owner: Option<String>,
    pub group: Option<String>,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for Directory {
    fn id_hint(&self) -> String {
        format!("directory:{}", self.path.display())
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let want_uid = match &self.owner {
            Some(name) => Some(lookup_uid(name).await?),
            None => None,
        };
        let want_gid = match &self.group {
            Some(name) => Some(lookup_gid(name).await?),
            None => None,
        };

        let current = read_metadata(&self.path).await?;
        let needs_create = current.is_none();
        let needs_mode_fix = match (&self.mode, &current) {
            (Some(want), Some(meta)) => meta.permissions().mode() & 0o7777 != want.mode() & 0o7777,
            _ => false,
        };
        let needs_chown = current.as_ref().map_or_else(
            || want_uid.is_some() || want_gid.is_some(),
            |meta| {
                want_uid.is_some_and(|w| meta.uid() != w)
                    || want_gid.is_some_and(|w| meta.gid() != w)
            },
        );

        if !needs_create && !needs_mode_fix && !needs_chown {
            return Ok(Changed::No);
        }

        if env.is_dry_run() {
            debug!(
                path = %self.path.display(),
                create = needs_create,
                mode_fix = needs_mode_fix,
                chown = needs_chown,
                "dry-run: would adjust directory",
            );
            return Ok(Changed::Yes);
        }

        if needs_create {
            fs::create_dir_all(&self.path).await.map_err(|e| {
                BackendError::with_source(BACKEND, format!("mkdir `{}`", self.path.display()), e)
            })?;
        }
        if let Some(mode) = self.mode.clone() {
            fs::set_permissions(&self.path, mode).await.map_err(|e| {
                BackendError::with_source(BACKEND, format!("chmod `{}`", self.path.display()), e)
            })?;
        }
        if want_uid.is_some() || want_gid.is_some() {
            std::os::unix::fs::chown(&self.path, want_uid, want_gid).map_err(|e| {
                BackendError::with_source(BACKEND, format!("chown `{}`", self.path.display()), e)
            })?;
        }
        Ok(Changed::Yes)
    }
}

async fn read_metadata(path: &Path) -> Result<Option<std::fs::Metadata>, BackendError> {
    match fs::metadata(path).await {
        Ok(meta) => {
            if !meta.is_dir() {
                return Err(BackendError::new(
                    BACKEND,
                    format!("`{}` exists but is not a directory", path.display()),
                ));
            }
            Ok(Some(meta))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(BackendError::with_source(
            BACKEND,
            format!("stat `{}`", path.display()),
            e,
        )),
    }
}

async fn lookup_uid(name: &str) -> Result<u32, BackendError> {
    let output = Command::new("getent")
        .args(["passwd", name])
        .output()
        .await
        .map_err(|e| {
            BackendError::with_source(BACKEND, format!("spawn getent passwd {name}"), e)
        })?;
    if !output.status.success() {
        return Err(BackendError::new(
            BACKEND,
            format!("user `{name}` not found via getent passwd"),
        ));
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() < 3 {
        return Err(BackendError::new(
            BACKEND,
            format!("malformed getent passwd output for `{name}`: {line}"),
        ));
    }
    parts[2].parse().map_err(|_| {
        BackendError::new(
            BACKEND,
            format!(
                "getent passwd uid for `{name}` is not numeric: {}",
                parts[2]
            ),
        )
    })
}

async fn lookup_gid(name: &str) -> Result<u32, BackendError> {
    let output = Command::new("getent")
        .args(["group", name])
        .output()
        .await
        .map_err(|e| BackendError::with_source(BACKEND, format!("spawn getent group {name}"), e))?;
    if !output.status.success() {
        return Err(BackendError::new(
            BACKEND,
            format!("group `{name}` not found via getent group"),
        ));
    }
    let line = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = line.split(':').collect();
    if parts.len() < 3 {
        return Err(BackendError::new(
            BACKEND,
            format!("malformed getent group output for `{name}`: {line}"),
        ));
    }
    parts[2].parse().map_err(|_| {
        BackendError::new(
            BACKEND,
            format!("getent group gid for `{name}` is not numeric: {}", parts[2]),
        )
    })
}
