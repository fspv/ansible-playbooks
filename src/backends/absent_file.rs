use std::path::PathBuf;

use async_trait::async_trait;
use tokio::fs;
use tracing::debug;

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "absent-file";

/// Ensure a file does not exist at `path`. Companion to
/// [`File`](super::file::File) — using a separate type rather than an
/// `absent: bool` flag keeps each backend's fields all-relevant.
#[derive(Debug, Default)]
pub struct AbsentFile {
    pub path: PathBuf,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for AbsentFile {
    fn id_hint(&self) -> String {
        format!("absent-file:{}", self.path.display())
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        match fs::metadata(&self.path).await {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                debug!(path = %self.path.display(), "file already absent");
                Ok(Changed::No)
            }
            Err(e) => Err(BackendError::with_source(
                BACKEND,
                format!("stat `{}`", self.path.display()),
                e,
            )),
            Ok(_) => {
                if env.is_dry_run() {
                    debug!(path = %self.path.display(), "dry-run: would remove file");
                    return Ok(Changed::Yes);
                }
                fs::remove_file(&self.path).await.map_err(|e| {
                    BackendError::with_source(
                        BACKEND,
                        format!("remove `{}`", self.path.display()),
                        e,
                    )
                })?;
                Ok(Changed::Yes)
            }
        }
    }
}
