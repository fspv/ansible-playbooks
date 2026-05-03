use crate::backends::apt_package::AptPackage;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let pkg_ids: Vec<_> = ["python3-pip", "python3-virtualenv"]
        .iter()
        .map(|name| {
            ctx.plan.add(AptPackage {
                name: (*name).to_string(),
                deps: vec![apt_ready],
                ..Default::default()
            })
        })
        .collect();

    ctx.plan.add(Marker {
        name: "pip:ready".to_string(),
        deps: pkg_ids,
        ..Default::default()
    })
}
