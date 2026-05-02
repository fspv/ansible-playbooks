use async_trait::async_trait;

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource, ResourceId, Skip};

/// No-op aggregation point used to expose "this bundle is fully ready" as a
/// single `ResourceId`.
///
/// Bundles return a `Marker` whose `deps` list every resource they added.
/// Downstream bundles take the marker's id as a single dependency instead of
/// carrying around `Vec<ResourceId>` of every upstream resource. The marker
/// itself does nothing on converge — it sits in the DAG so its dependencies
/// must finish before any of *its* dependents can start.
#[derive(Debug, Default)]
pub struct Marker {
    pub name: String,
    pub deps: Vec<ResourceId>,
    pub skip_when: Skip,
}

#[async_trait]
impl Resource for Marker {
    fn id_hint(&self) -> String {
        format!("marker:{}", self.name)
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, _env: &Env) -> Result<Changed, BackendError> {
        Ok(Changed::No)
    }
}
