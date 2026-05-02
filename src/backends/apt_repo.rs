use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use async_trait::async_trait;

use crate::backends::file::File;
use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

/// An apt repository — a `sources.list.d` entry plus an optional keyring.
///
/// Paths are derived from `name` using the modern (Ubuntu 24.04+) convention:
/// `/etc/apt/sources.list.d/{name}.list` and `/etc/apt/keyrings/{name}.asc`.
/// Reference the keyring from inside `list_content` via
/// `[signed-by=/etc/apt/keyrings/{name}.asc]`. Both files are written
/// atomically through the File backend, so the apt-package batcher's
/// `apt-get update` step (which runs before any install) sees the new repo
/// on the next level.
#[derive(Debug, Default)]
pub struct AptRepo {
    pub name: String,
    pub list_content: String,
    pub key_content: Option<String>,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for AptRepo {
    fn id_hint(&self) -> String {
        format!("apt-repo:{}", self.name)
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let mut any_change = false;

        if let Some(key_content) = &self.key_content {
            let key_file = File {
                path: PathBuf::from(format!("/etc/apt/keyrings/{}.asc", self.name)),
                content: key_content.clone(),
                mode: Some(Permissions::from_mode(0o644)),
                ..Default::default()
            };
            if key_file.converge_one(env).await? == Changed::Yes {
                any_change = true;
            }
        }

        let list_file = File {
            path: PathBuf::from(format!("/etc/apt/sources.list.d/{}.list", self.name)),
            content: self.list_content.clone(),
            mode: Some(Permissions::from_mode(0o644)),
            ..Default::default()
        };
        if list_file.converge_one(env).await? == Changed::Yes {
            any_change = true;
        }

        Ok(if any_change {
            Changed::Yes
        } else {
            Changed::No
        })
    }
}
