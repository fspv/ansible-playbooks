use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Top-level bundle that mirrors common-devserver.yml — `common` + `devserver`.
// Selecting this bundle is the equivalent of `bootstrap.sh common-devserver.yml`.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let common_ready = ctx.common();
    let devserver_ready = ctx.devserver();

    ctx.plan.add(Marker {
        name: "common_devserver:ready".to_string(),
        deps: vec![common_ready, devserver_ready],
        ..Default::default()
    })
}
