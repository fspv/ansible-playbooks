use crate::backends::apt_package::AptPackage;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/common-tools/vars/main.yml. The arch-specific list (syslinux,
// libsqlite3-dev, sqlite3 — only meaningful on x86_64) is included
// unconditionally for now: this framework targets x86_64 hosts. When we
// start running on aarch64, gate the second list on a future
// `Env::architecture()` accessor.
//
// Every package depends on the `apt:ready` marker so it installs only after
// the apt bundle's conf.d files and bootstrap packages have converged. We
// fetch the apt marker via `ctx.apt()` — the context's cache ensures apt is
// only built once even if other bundles also call `ctx.apt()`.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let pkg_ids: Vec<_> = [
        "linux-tools-common",
        "uidmap",
        "syslinux",
        "libsqlite3-dev",
        "sqlite3",
    ]
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
        name: "common_tools:ready".to_string(),
        deps: pkg_ids,
        ..Default::default()
    })
}
