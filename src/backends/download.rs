use std::ffi::OsString;
use std::fs::Permissions;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "download";

/// Fetch a URL via HTTPS and write the body to a path.
///
/// Idempotency model: every converge issues a GET, compares the response
/// body byte-for-byte against the on-disk file, and writes only if they
/// differ. That means the network is hit on every run (~100ms for small
/// files like GPG keys) but drift is caught when the upstream rotates a
/// key. In dry-run, the GET still happens (it's read-only) but no write
/// occurs even when bytes differ.
#[derive(Debug, Default)]
pub struct Download {
    pub url: String,
    pub path: PathBuf,
    pub mode: Option<Permissions>,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for Download {
    fn id_hint(&self) -> String {
        format!("download:{}", self.path.display())
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let body = fetch(&self.url).await?;

        let current = read_existing(&self.path).await?;
        if current.as_deref() == Some(body.as_slice()) {
            debug!(path = %self.path.display(), "download already matches upstream");
            return Ok(Changed::No);
        }

        if env.is_dry_run() {
            debug!(url = %self.url, path = %self.path.display(), "dry-run: would write fetched bytes");
            return Ok(Changed::Yes);
        }

        ensure_parent(&self.path).await?;
        let tmp = sibling_temp(&self.path);
        write_atomic(&tmp, &body).await?;

        if let Some(mode) = self.mode.clone() {
            fs::set_permissions(&tmp, mode).await.map_err(|e| {
                BackendError::with_source(BACKEND, format!("chmod `{}`", tmp.display()), e)
            })?;
        }

        fs::rename(&tmp, &self.path).await.map_err(|e| {
            BackendError::with_source(
                BACKEND,
                format!("rename `{}` -> `{}`", tmp.display(), self.path.display()),
                e,
            )
        })?;
        info!(url = %self.url, path = %self.path.display(), bytes = body.len(), "downloaded");
        Ok(Changed::Yes)
    }
}

async fn fetch(url: &str) -> Result<Vec<u8>, BackendError> {
    let resp = reqwest::get(url)
        .await
        .map_err(|e| BackendError::with_source(BACKEND, format!("GET `{url}`"), e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(BackendError::new(
            BACKEND,
            format!("GET `{url}` returned HTTP {status}"),
        ));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| BackendError::with_source(BACKEND, format!("read body of `{url}`"), e))?;
    Ok(bytes.to_vec())
}

async fn read_existing(path: &Path) -> Result<Option<Vec<u8>>, BackendError> {
    match fs::read(path).await {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(BackendError::with_source(
            BACKEND,
            format!("read `{}`", path.display()),
            e,
        )),
    }
}

async fn ensure_parent(path: &Path) -> Result<(), BackendError> {
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

fn sibling_temp(path: &Path) -> PathBuf {
    let mut tmp: OsString = path.as_os_str().to_owned();
    tmp.push(".tmp.host-setup");
    PathBuf::from(tmp)
}

async fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), BackendError> {
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
