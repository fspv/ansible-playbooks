use std::ffi::OsString;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::debug;

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "file";

/// Manage a file on disk: ensure `content` is written with optional `mode`.
/// Use [`AbsentFile`](super::absent_file::AbsentFile) to ensure a file
/// does not exist.
#[derive(Debug, Default)]
pub struct File {
    pub path: PathBuf,
    pub content: String,
    pub mode: Option<Permissions>,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for File {
    fn id_hint(&self) -> String {
        format!("file:{}", self.path.display())
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let current_content = read_existing_content(&self.path).await?;
        let content_matches = current_content.as_deref() == Some(self.content.as_str());
        let mode_matches = self.mode_matches_disk().await?;

        if content_matches && mode_matches {
            debug!(path = %self.path.display(), "file already in desired state");
            return Ok(Changed::No);
        }

        if !content_matches {
            super::log_file_diff(
                &self.path,
                current_content.as_deref().unwrap_or(""),
                &self.content,
            );
        }

        if env.is_dry_run() {
            debug!(path = %self.path.display(), "dry-run: would write file");
            return Ok(Changed::Yes);
        }

        ensure_parent_directory(&self.path).await?;

        let tmp_path = sibling_temp_path(&self.path);
        write_full(&tmp_path, self.content.as_bytes()).await?;

        if let Some(mode) = self.mode.clone() {
            fs::set_permissions(&tmp_path, mode).await.map_err(|e| {
                BackendError::with_source(
                    BACKEND,
                    format!("set permissions on `{}`", tmp_path.display()),
                    e,
                )
            })?;
        }

        fs::rename(&tmp_path, &self.path).await.map_err(|e| {
            BackendError::with_source(
                BACKEND,
                format!(
                    "rename `{}` -> `{}`",
                    tmp_path.display(),
                    self.path.display()
                ),
                e,
            )
        })?;
        Ok(Changed::Yes)
    }
}

impl File {
    async fn mode_matches_disk(&self) -> Result<bool, BackendError> {
        let Some(want) = self.mode.as_ref() else {
            return Ok(true);
        };
        let want_bits = want.mode() & 0o7777;
        match fs::metadata(&self.path).await {
            Ok(meta) => Ok(meta.permissions().mode() & 0o7777 == want_bits),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(BackendError::with_source(
                BACKEND,
                format!("stat `{}`", self.path.display()),
                e,
            )),
        }
    }
}

async fn read_existing_content(path: &Path) -> Result<Option<String>, BackendError> {
    match fs::read_to_string(path).await {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(BackendError::with_source(
            BACKEND,
            format!("read `{}`", path.display()),
            e,
        )),
    }
}

async fn ensure_parent_directory(path: &Path) -> Result<(), BackendError> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    if parent.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(parent).await.map_err(|e| {
        BackendError::with_source(
            BACKEND,
            format!("create directory `{}`", parent.display()),
            e,
        )
    })
}

fn sibling_temp_path(path: &Path) -> PathBuf {
    let mut tmp: OsString = path.as_os_str().to_owned();
    tmp.push(".tmp.host-setup");
    PathBuf::from(tmp)
}

async fn write_full(path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
    let mut f = fs::File::create(path).await.map_err(|e| {
        BackendError::with_source(BACKEND, format!("create `{}`", path.display()), e)
    })?;
    f.write_all(bytes).await.map_err(|e| {
        BackendError::with_source(BACKEND, format!("write `{}`", path.display()), e)
    })?;
    f.flush().await.map_err(|e| {
        BackendError::with_source(BACKEND, format!("flush `{}`", path.display()), e)
    })?;
    Ok(())
}
