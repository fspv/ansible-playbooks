use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/common (pure meta — pulls in common-tweaks and common-tools).

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let common_tweaks_ready = ctx.common_tweaks();
    let common_tools_ready = ctx.common_tools();

    ctx.plan.add(Marker {
        name: "common:ready".to_string(),
        deps: vec![common_tweaks_ready, common_tools_ready],
        ..Default::default()
    })
}
