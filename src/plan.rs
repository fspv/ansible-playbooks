use std::sync::Arc;

use crate::resource::{Resource, ResourceId};

#[derive(Debug, Default)]
pub struct Plan {
    nodes: Vec<Node>,
}

#[derive(Debug)]
pub struct Node {
    pub id: ResourceId,
    pub resource: Arc<dyn Resource>,
}

impl Plan {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add<R: Resource>(&mut self, resource: R) -> ResourceId {
        let id = ResourceId(self.nodes.len());
        self.nodes.push(Node {
            id,
            resource: Arc::new(resource),
        });
        id
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub(crate) fn nodes(&self) -> &[Node] {
        &self.nodes
    }
}
