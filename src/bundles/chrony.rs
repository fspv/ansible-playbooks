use crate::backends::apt_package::AptPackage;
use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/ntp/ semantically — installs an NTP implementation and
// keeps the daemon enabled and running. The legacy role used the `ntp`
// package; modern Ubuntu (24.04+) ships chrony as the default time daemon,
// so this bundle installs `chrony` and manages `chrony.service` instead.
// The bundle is exposed as `--bundle chrony` (renamed from the legacy
// `--bundle ntp`) to match what's actually being installed.
//
// Service management is skipped in containers — the host time daemon is
// authoritative there, and `systemctl` against a unit owned by PID 1 of
// the host doesn't work inside one anyway.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let pkg = ctx.plan.add(AptPackage {
        name: "chrony".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let service = ctx.plan.add(Service {
        name: "chrony.service".to_string(),
        enabled: true,
        started: true,
        deps: vec![pkg],
        skip_when: Skip::InContainer,
        ..Default::default()
    });

    ctx.plan.add(Marker {
        name: "chrony:ready".to_string(),
        deps: vec![pkg, service],
        ..Default::default()
    })
}
