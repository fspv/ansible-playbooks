use crate::backends::apt_package::AptPackage;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/ntp/tasks/packages.yml semantically — installs an NTP
// implementation. The legacy role used the `ntp` package; modern Ubuntu
// (24.04+) ships chrony as the default time daemon, so this bundle
// installs `chrony` instead. The bundle is exposed as `--bundle chrony`
// (renamed from the legacy `--bundle ntp`) to match what's actually being
// installed.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let pkg = ctx.plan.add(AptPackage {
        name: "chrony".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    });

    ctx.plan.add(Marker {
        name: "chrony:ready".to_string(),
        deps: vec![pkg],
        ..Default::default()
    })
}
