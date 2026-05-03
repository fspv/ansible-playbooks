use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/devserver — meta-only, pulls in ubuntu-devserver.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let ubuntu_devserver_ready = ctx.ubuntu_devserver();

    ctx.plan.add(Marker {
        name: "devserver:ready".to_string(),
        deps: vec![ubuntu_devserver_ready],
        ..Default::default()
    })
}
