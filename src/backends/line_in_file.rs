use std::ffi::OsString;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use regex::Regex;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::debug;

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

const BACKEND: &str = "line-in-file";

/// Ensure a single line is present in an existing file.
///
/// If any line in the file matches `regex`, every match is replaced with
/// `line`; otherwise `line` is appended at the end (with a trailing newline
/// inserted first if the file doesn't end with one). Refuses to create the
/// file if it's missing — that's a separate concern; combine with a `File`
/// resource if you need both write-and-edit.
#[derive(Debug, Default)]
pub struct LineInFile {
    pub path: PathBuf,
    pub regex: String,
    pub line: String,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for LineInFile {
    fn id_hint(&self) -> String {
        format!("line-in-file:{}", self.path.display())
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError> {
        let current = fs::read_to_string(&self.path).await.map_err(|e| {
            BackendError::with_source(BACKEND, format!("read `{}`", self.path.display()), e)
        })?;

        let pattern = Regex::new(&self.regex).map_err(|e| {
            BackendError::with_source(BACKEND, format!("compile regex `{}`", self.regex), e)
        })?;

        let new_content = compute_new_content(&current, &pattern, &self.line);

        if new_content == current {
            debug!(path = %self.path.display(), "line-in-file already in desired state");
            return Ok(Changed::No);
        }

        super::log_file_diff(&self.path, &current, &new_content);

        if env.is_dry_run() {
            debug!(path = %self.path.display(), "dry-run: would update line-in-file");
            return Ok(Changed::Yes);
        }

        let tmp_path = sibling_temp_path(&self.path);
        write_full(&tmp_path, new_content.as_bytes()).await?;
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

fn compute_new_content(current: &str, pattern: &Regex, line: &str) -> String {
    let any_match = current.lines().any(|l| pattern.is_match(l));
    if any_match {
        let trailing_newline = current.ends_with('\n');
        let mut out = String::with_capacity(current.len());
        for src_line in current.lines() {
            if pattern.is_match(src_line) {
                out.push_str(line);
            } else {
                out.push_str(src_line);
            }
            out.push('\n');
        }
        if !trailing_newline {
            out.pop();
        }
        out
    } else {
        let mut out = String::with_capacity(current.len() + line.len() + 1);
        out.push_str(current);
        if !current.is_empty() && !current.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(line);
        out.push('\n');
        out
    }
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
