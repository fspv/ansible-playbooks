use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use async_trait::async_trait;

use crate::backends::file::File;
use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

/// An apt repository — writes `/etc/apt/sources.list.d/{name}.list`.
///
/// The path is derived from `name` using the modern (Ubuntu 24.04+)
/// convention. Reference any keyring from inside `list_content` via
/// `[signed-by=/etc/apt/keyrings/{name}.asc]`; the actual key file is the
/// caller's responsibility — typically a `Download` resource that fetches
/// it from the upstream's HTTPS endpoint, set as a `dep` of this `AptRepo`.
#[derive(Debug, Default)]
pub struct AptRepo {
    pub name: String,
    pub list_content: String,
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
        let list_file = File {
            path: PathBuf::from(format!("/etc/apt/sources.list.d/{}.list", self.name)),
            content: self.list_content.clone(),
            mode: Some(Permissions::from_mode(0o644)),
            ..Default::default()
        };
        list_file.converge_one(env).await
    }
}
