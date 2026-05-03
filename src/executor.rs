use std::collections::HashMap;
use std::sync::Arc;

use tokio::task::JoinSet;
use tracing::{debug, info};

use crate::backends::absent_apt_package::AbsentAptPackageBatcher;
use crate::backends::apt_package::AptPackageBatcher;
use crate::batcher::Batcher;
use crate::env::Env;
use crate::error::Error;
use crate::plan::Plan;
use crate::resource::{BatchFamily, Changed, Resource, ResourceId};

#[derive(Debug, Default)]
pub struct Report {
    pub outcomes: Vec<ResourceOutcome>,
}

impl Report {
    #[must_use]
    pub fn count(&self, kind: Changed) -> usize {
        self.outcomes.iter().filter(|o| o.changed == kind).count()
    }
}

#[derive(Debug, Clone)]
pub struct ResourceOutcome {
    pub id: ResourceId,
    pub id_hint: String,
    pub changed: Changed,
}

#[derive(Debug)]
pub struct Executor {
    plan: Plan,
    batchers: HashMap<BatchFamily, Arc<dyn Batcher>>,
}

impl Executor {
    #[must_use]
    pub fn new(plan: Plan) -> Self {
        let mut batchers: HashMap<BatchFamily, Arc<dyn Batcher>> = HashMap::new();
        batchers.insert(BatchFamily::AptPackage, Arc::new(AptPackageBatcher));
        batchers.insert(
            BatchFamily::AbsentAptPackage,
            Arc::new(AbsentAptPackageBatcher),
        );
        Self { plan, batchers }
    }

    /// Run the full plan to convergence.
    ///
    /// # Errors
    /// Returns the first error encountered: a [`Error::PlanCycle`] /
    /// [`Error::PlanReferencesUnknownResource`] if the DAG is malformed,
    /// or a wrapped [`Error::Backend`] from any resource that fails to
    /// converge.
    pub async fn run(self, env: Arc<Env>) -> Result<Report, Error> {
        let levels = self.compute_topo_levels()?;
        let mut report = Report::default();

        for (level_idx, level) in levels.iter().enumerate() {
            debug!(
                level = level_idx,
                resource_count = level.len(),
                "starting level"
            );
            self.run_level(level, &env, &mut report).await?;
        }

        info!(
            total = report.outcomes.len(),
            changed_or_would_change = report.count(Changed::Yes),
            unchanged = report.count(Changed::No),
            skipped = report.count(Changed::Skipped),
            dry_run = env.is_dry_run(),
            "plan run complete",
        );
        Ok(report)
    }

    fn compute_topo_levels(&self) -> Result<Vec<Vec<usize>>, Error> {
        let nodes = self.plan.nodes();
        let n = nodes.len();
        let mut in_degree = vec![0usize; n];
        let mut successors: Vec<Vec<usize>> = vec![Vec::new(); n];

        for (i, node) in nodes.iter().enumerate() {
            for dep_id in node.resource.deps() {
                let dep_idx = dep_id.0;
                if dep_idx >= n {
                    return Err(Error::PlanReferencesUnknownResource {
                        from_resource: node.resource.id_hint(),
                        unknown_dep_index: dep_idx,
                    });
                }
                successors[dep_idx].push(i);
                in_degree[i] += 1;
            }
        }

        let mut levels: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut visited = 0usize;

        while !current.is_empty() {
            let mut next: Vec<usize> = Vec::new();
            for &i in &current {
                visited += 1;
                for &j in &successors[i] {
                    in_degree[j] -= 1;
                    if in_degree[j] == 0 {
                        next.push(j);
                    }
                }
            }
            levels.push(std::mem::take(&mut current));
            current = next;
        }

        if visited != n {
            let stuck = nodes
                .iter()
                .enumerate()
                .find(|(i, _)| in_degree[*i] > 0)
                .map_or_else(
                    || "<unknown>".to_string(),
                    |(_, node)| node.resource.id_hint(),
                );
            return Err(Error::PlanCycle {
                resource_id_hint: stuck,
            });
        }

        Ok(levels)
    }

    async fn run_level(
        &self,
        level: &[usize],
        env: &Arc<Env>,
        report: &mut Report,
    ) -> Result<(), Error> {
        let nodes = self.plan.nodes();
        let mut by_family: HashMap<Option<BatchFamily>, Vec<usize>> = HashMap::new();

        for &i in level {
            let node = &nodes[i];
            if node.resource.skip_when().evaluate(env) {
                debug!(resource = %node.resource.id_hint(), "skipped (skip_when matched)");
                report.outcomes.push(ResourceOutcome {
                    id: node.id,
                    id_hint: node.resource.id_hint(),
                    changed: Changed::Skipped,
                });
                continue;
            }
            by_family
                .entry(node.resource.batch_family())
                .or_default()
                .push(i);
        }

        let mut joinset: JoinSet<Result<Vec<ResourceOutcome>, Error>> = JoinSet::new();

        for (family, indices) in by_family {
            let resources: Vec<Arc<dyn Resource>> = indices
                .iter()
                .map(|&i| Arc::clone(&nodes[i].resource))
                .collect();
            let ids: Vec<(ResourceId, String)> = indices
                .iter()
                .map(|&i| (nodes[i].id, nodes[i].resource.id_hint()))
                .collect();
            let env_for_task = Arc::clone(env);

            let batcher = family.and_then(|f| self.batchers.get(&f).cloned());
            if let Some(batcher) = batcher {
                joinset.spawn(
                    async move { run_batched(batcher, resources, ids, &env_for_task).await },
                );
            } else {
                for (resource, (id, id_hint)) in resources.into_iter().zip(ids) {
                    let env_one = Arc::clone(&env_for_task);
                    joinset.spawn(async move { run_one(resource, id, id_hint, &env_one).await });
                }
            }
        }

        while let Some(joined) = joinset.join_next().await {
            let outcomes = joined.map_err(|e| Error::TaskPanicked {
                context: format!("task join failed: {e}"),
            })??;
            report.outcomes.extend(outcomes);
        }

        Ok(())
    }
}

async fn run_batched(
    batcher: Arc<dyn Batcher>,
    resources: Vec<Arc<dyn Resource>>,
    ids: Vec<(ResourceId, String)>,
    env: &Env,
) -> Result<Vec<ResourceOutcome>, Error> {
    debug!(
        family = batcher.family_name(),
        size = resources.len(),
        "dispatching batch",
    );
    let changes = batcher
        .converge_batch(&resources, env)
        .await
        .map_err(|source| Error::Backend {
            resource: format!("batch:{}", batcher.family_name()),
            source,
        })?;
    if changes.len() != ids.len() {
        return Err(Error::Backend {
            resource: format!("batch:{}", batcher.family_name()),
            source: crate::error::BackendError::new(
                "executor",
                format!(
                    "batcher returned {} outcomes for {} inputs",
                    changes.len(),
                    ids.len(),
                ),
            ),
        });
    }
    Ok(ids
        .into_iter()
        .zip(changes)
        .map(|((id, id_hint), changed)| ResourceOutcome {
            id,
            id_hint,
            changed,
        })
        .collect())
}

async fn run_one(
    resource: Arc<dyn Resource>,
    id: ResourceId,
    id_hint: String,
    env: &Env,
) -> Result<Vec<ResourceOutcome>, Error> {
    let changed = resource
        .converge_one(env)
        .await
        .map_err(|source| Error::Backend {
            resource: id_hint.clone(),
            source,
        })?;
    Ok(vec![ResourceOutcome {
        id,
        id_hint,
        changed,
    }])
}
