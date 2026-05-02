use std::sync::Arc;

use async_trait::async_trait;

use crate::env::Env;
use crate::error::BackendError;
use crate::resource::{Changed, Resource};

#[async_trait]
pub trait Batcher: std::fmt::Debug + Send + Sync {
    fn family_name(&self) -> &'static str;

    async fn converge_batch(
        &self,
        resources: &[Arc<dyn Resource>],
        env: &Env,
    ) -> Result<Vec<Changed>, BackendError>;
}
