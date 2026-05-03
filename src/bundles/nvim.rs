use crate::backends::apt_package::AptPackage;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let pkg = ctx.plan.add(AptPackage {
        name: "neovim".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    });

    ctx.plan.add(Marker {
        name: "nvim:ready".to_string(),
        deps: vec![pkg],
        ..Default::default()
    })
}
