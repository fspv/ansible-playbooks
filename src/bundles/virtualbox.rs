use crate::backends::apt_package::AptPackage;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/virtualbox/. Always installs the `virtualbox` package
// (matches the legacy unconditional `apt: name=virtualbox`). Guest
// additions (`virtualbox-guest-utils`, `virtualbox-guest-x11`) are only
// installed when the host is itself running inside a VirtualBox VM, which
// the legacy role detected via `lspci | grep VirtualBox`. We do the same
// probe once at startup in `Env::detect` and read the result here.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let mut all = vec![ctx.plan.add(AptPackage {
        name: "virtualbox".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    })];

    if ctx.env.is_in_virtualbox() {
        for name in ["virtualbox-guest-utils", "virtualbox-guest-x11"] {
            all.push(ctx.plan.add(AptPackage {
                name: name.to_string(),
                deps: vec![apt_ready],
                ..Default::default()
            }));
        }
    }

    ctx.plan.add(Marker {
        name: "virtualbox:ready".to_string(),
        deps: all,
        ..Default::default()
    })
}
