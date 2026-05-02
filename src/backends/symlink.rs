use std::path::PathBuf;

use async_trait::async_trait;
use tokio::fs;
use tracing::debug;

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "symlink";

#[derive(Debug, Default)]
pub struct Symlink {
    pub path: PathBuf,
    pub target: PathBuf,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for Symlink {
    fn id_hint(&self) -> String {
        format!("symlink:{}", self.path.display())
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let current = read_link_target(&self.path).await?;
        if current.as_ref() == Some(&self.target) {
            return Ok(Changed::No);
        }

        if env.is_dry_run() {
            debug!(
                path = %self.path.display(),
                target = %self.target.display(),
                current = ?current,
                "dry-run: would create/replace symlink",
            );
            return Ok(Changed::Yes);
        }

        // Replace whatever is there. fs::symlink fails if the path exists, so
        // remove first if present.
        match fs::symlink_metadata(&self.path).await {
            Ok(_) => fs::remove_file(&self.path).await.map_err(|e| {
                BackendError::with_source(
                    BACKEND,
                    format!("remove existing path `{}`", self.path.display()),
                    e,
                )
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(BackendError::with_source(
                    BACKEND,
                    format!("stat `{}`", self.path.display()),
                    e,
                ))
            }
        }

        fs::symlink(&self.target, &self.path).await.map_err(|e| {
            BackendError::with_source(
                BACKEND,
                format!(
                    "create symlink `{}` -> `{}`",
                    self.path.display(),
                    self.target.display()
                ),
                e,
            )
        })?;
        Ok(Changed::Yes)
    }
}

async fn read_link_target(path: &std::path::Path) -> Result<Option<PathBuf>, BackendError> {
    match fs::symlink_metadata(path).await {
        Ok(meta) if meta.file_type().is_symlink() => {
            fs::read_link(path).await.map(Some).map_err(|e| {
                BackendError::with_source(BACKEND, format!("read_link `{}`", path.display()), e)
            })
        }
        Ok(_) => Err(BackendError::new(
            BACKEND,
            format!(
                "`{}` exists but is not a symlink — refusing to clobber",
                path.display()
            ),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(BackendError::with_source(
            BACKEND,
            format!("stat `{}`", path.display()),
            e,
        )),
    }
}
