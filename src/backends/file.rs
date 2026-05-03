// uid/gid are unix terms-of-art that intentionally appear as a pair. Pedantic
// `similar_names` flags `want_uid`/`want_gid` as too close — they're meant to
// be parallel; renaming them harms readability more than it helps.
#![allow(clippy::similar_names)]

use std::ffi::OsString;
use std::fs::Permissions;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tracing::debug;

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "file";

/// Manage a file on disk: ensure `content` is written with optional `mode`,
/// `owner`, `group`. Use [`AbsentFile`](super::absent_file::AbsentFile) to
/// ensure a file does not exist.
#[derive(Debug, Default)]
pub struct File {
    pub path: PathBuf,
    pub content: String,
    pub mode: Option<Permissions>,
    pub owner: Option<String>,
    pub group: Option<String>,
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
        let want_uid = match &self.owner {
            Some(name) => Some(lookup_uid(name).await?),
            None => None,
        };
        let want_gid = match &self.group {
            Some(name) => Some(lookup_gid(name).await?),
            None => None,
        };

        let current_content = read_existing_content(&self.path).await?;
        let content_matches = current_content.as_deref() == Some(self.content.as_str());
        let current_meta = fs::metadata(&self.path).await.ok();
        let mode_matches = self.mode.as_ref().is_none_or(|want| {
            current_meta
                .as_ref()
                .is_some_and(|m| m.permissions().mode() & 0o7777 == want.mode() & 0o7777)
        });
        let ownership_matches =
            current_meta.as_ref().is_some_and(|m| {
                want_uid.is_none_or(|w| m.uid() == w) && want_gid.is_none_or(|w| m.gid() == w)
            }) || (current_meta.is_none() && want_uid.is_none() && want_gid.is_none());

        if content_matches && mode_matches && ownership_matches {
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

        // If the only drift is ownership/mode, chown/chmod in place rather
        // than re-writing identical bytes.
        if content_matches {
            if let Some(mode) = self.mode.clone() {
                fs::set_permissions(&self.path, mode).await.map_err(|e| {
                    BackendError::with_source(
                        BACKEND,
                        format!("set permissions on `{}`", self.path.display()),
                        e,
                    )
                })?;
            }
            if want_uid.is_some() || want_gid.is_some() {
                std::os::unix::fs::chown(&self.path, want_uid, want_gid).map_err(|e| {
                    BackendError::with_source(
                        BACKEND,
                        format!("chown `{}`", self.path.display()),
                        e,
                    )
                })?;
            }
            return Ok(Changed::Yes);
        }

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

        if want_uid.is_some() || want_gid.is_some() {
            std::os::unix::fs::chown(&tmp_path, want_uid, want_gid).map_err(|e| {
                BackendError::with_source(BACKEND, format!("chown `{}`", tmp_path.display()), e)
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
