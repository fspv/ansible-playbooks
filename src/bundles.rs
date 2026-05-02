use std::collections::HashMap;

use crate::config::Config;
use crate::env::Env;
use crate::plan::Plan;
use crate::resource::ResourceId;

pub mod apt;
pub mod common_tools;
pub mod users;

/// Mutable working context threaded through every bundle's `build`.
///
/// It owns the `Plan` being assembled and exposes typed methods (`ctx.apt()`,
/// `ctx.users()`, ...) that lazily build each bundle, returning its `Marker`
/// `ResourceId`. Each bundle is built **at most once** per context — the memo
/// cache deduplicates so multiple downstream bundles can call `ctx.apt()` and
/// all get the same id without re-registering apt's resources.
///
/// Bundle dependencies live inside each bundle's `build` body: when a bundle
/// needs another, it just calls the corresponding method on the context.
/// There is no separate "declared deps" table to keep in sync.
#[derive(Debug)]
pub struct Context<'a> {
    pub plan: &'a mut Plan,
    pub env: &'a Env,
    pub config: &'a Config,
    cache: HashMap<&'static str, ResourceId>,
}

impl<'a> Context<'a> {
    pub fn new(plan: &'a mut Plan, env: &'a Env, config: &'a Config) -> Self {
        Self {
            plan,
            env,
            config,
            cache: HashMap::new(),
        }
    }

    pub fn apt(&mut self) -> ResourceId {
        self.memoized("apt", apt::build)
    }

    pub fn common_tools(&mut self) -> ResourceId {
        self.memoized("common_tools", common_tools::build)
    }

    pub fn users(&mut self) -> ResourceId {
        self.memoized("users", users::build)
    }

    fn memoized(&mut self, key: &'static str, build: fn(&mut Self) -> ResourceId) -> ResourceId {
        if let Some(id) = self.cache.get(key) {
            return *id;
        }
        let id = build(self);
        self.cache.insert(key, id);
        id
    }
}
