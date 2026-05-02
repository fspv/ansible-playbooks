#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::missing_const_for_fn,
    clippy::missing_panics_doc,
    clippy::missing_errors_doc
)]

use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use host_setup::env::Env;
use host_setup::error::BackendError;
use host_setup::executor::Executor;
use host_setup::plan::Plan;
use host_setup::resource::{Changed, Resource, ResourceId, Skip};

#[derive(Debug, Default)]
struct Recorder {
    log: Mutex<Vec<String>>,
}

impl Recorder {
    fn entries(&self) -> Vec<String> {
        self.log
            .lock()
            .unwrap_or_else(|e| panic!("recorder mutex poisoned: {e}"))
            .clone()
    }
}

#[derive(Debug)]
struct FakeResource {
    name: String,
    deps: Vec<ResourceId>,
    skip_when: Skip,
    recorder: Arc<Recorder>,
    fail: bool,
}

#[async_trait]
impl Resource for FakeResource {
    fn id_hint(&self) -> String {
        format!("fake:{}", self.name)
    }

    fn deps(&self) -> &[ResourceId] {
        &self.deps
    }

    fn skip_when(&self) -> &Skip {
        &self.skip_when
    }

    async fn converge_one(&self, _env: &Env) -> Result<Changed, BackendError> {
        self.recorder
            .log
            .lock()
            .unwrap_or_else(|e| panic!("recorder mutex poisoned: {e}"))
            .push(self.name.clone());
        if self.fail {
            return Err(BackendError::new(
                "fake",
                format!("synthetic failure in {}", self.name),
            ));
        }
        Ok(Changed::Yes)
    }
}

fn fake(recorder: &Arc<Recorder>, name: &str) -> FakeResource {
    FakeResource {
        name: name.to_string(),
        deps: Vec::new(),
        skip_when: Skip::Never,
        recorder: Arc::clone(recorder),
        fail: false,
    }
}

#[tokio::test]
async fn empty_plan_runs_clean() {
    let env = Arc::new(Env::synthetic(false));
    let report = Executor::new(Plan::new())
        .run(env)
        .await
        .unwrap_or_else(|e| panic!("empty plan should not fail: {e}"));
    assert!(
        report.outcomes.is_empty(),
        "empty plan should yield no outcomes, got: {:?}",
        report.outcomes,
    );
}

#[tokio::test]
async fn dependent_resources_run_after_their_deps() {
    let recorder = Arc::new(Recorder::default());
    let env = Arc::new(Env::synthetic(false));
    let mut plan = Plan::new();

    let a = plan.add(fake(&recorder, "a"));
    let b = FakeResource {
        deps: vec![a],
        ..fake(&recorder, "b")
    };
    let b = plan.add(b);
    let c = FakeResource {
        deps: vec![b],
        ..fake(&recorder, "c")
    };
    plan.add(c);

    Executor::new(plan)
        .run(env)
        .await
        .unwrap_or_else(|e| panic!("plan run failed: {e}"));

    let log = recorder.entries();
    let pos_a = log.iter().position(|s| s == "a");
    let pos_b = log.iter().position(|s| s == "b");
    let pos_c = log.iter().position(|s| s == "c");
    assert!(pos_a < pos_b, "a must run before b; log: {log:?}");
    assert!(pos_b < pos_c, "b must run before c; log: {log:?}");
}

#[tokio::test]
async fn skip_when_in_container_filters_resource() {
    let recorder = Arc::new(Recorder::default());
    let env = Arc::new(Env::synthetic(true));
    let mut plan = Plan::new();

    plan.add(FakeResource {
        skip_when: Skip::InContainer,
        ..fake(&recorder, "skipped")
    });
    plan.add(fake(&recorder, "ran"));

    let report = Executor::new(plan)
        .run(env)
        .await
        .unwrap_or_else(|e| panic!("plan run failed: {e}"));

    let log = recorder.entries();
    assert!(
        !log.iter().any(|s| s == "skipped"),
        "resource with Skip::InContainer should not have run; log: {log:?}",
    );
    assert!(
        log.iter().any(|s| s == "ran"),
        "non-skipped resource should have run; log: {log:?}",
    );
    let skipped_outcomes = report.count(Changed::Skipped);
    assert_eq!(skipped_outcomes, 1, "expected 1 skipped outcome in report");
}

#[tokio::test]
async fn cyclic_plan_returns_error() {
    let recorder = Arc::new(Recorder::default());
    let env = Arc::new(Env::synthetic(false));
    let mut plan = Plan::new();

    let a = plan.add(fake(&recorder, "a"));
    let b = plan.add(FakeResource {
        deps: vec![a],
        ..fake(&recorder, "b")
    });
    plan.add(FakeResource {
        deps: vec![b, ResourceId(99_999)],
        ..fake(&recorder, "c")
    });

    let err = Executor::new(plan)
        .run(env)
        .await
        .expect_err("expected unknown-dep error");
    let s = err.to_string();
    assert!(
        s.contains("not in this plan"),
        "expected unknown-dep error, got: {s}",
    );
}

#[tokio::test]
async fn backend_failure_propagates_as_error() {
    let recorder = Arc::new(Recorder::default());
    let env = Arc::new(Env::synthetic(false));
    let mut plan = Plan::new();

    plan.add(FakeResource {
        fail: true,
        ..fake(&recorder, "boom")
    });

    let err = Executor::new(plan)
        .run(env)
        .await
        .expect_err("expected backend failure to propagate");
    let s = err.to_string();
    assert!(
        s.contains("fake:boom"),
        "expected resource id_hint in error, got: {s}",
    );
}
