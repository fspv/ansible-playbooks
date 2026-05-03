use crate::backends::apt_package::AptPackage;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/common-tools. The legacy meta pulls pip / snapd / flatpak
// / yubico / nix. The role's own var-driven package list splits into a
// portable base (linux-tools-common, uidmap) and an x86_64-only extension
// (syslinux, libsqlite3-dev, sqlite3). On other architectures we install
// only the base list — add an arch-specific list per architecture as the
// need arises.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();
    let pip_ready = ctx.pip();
    let snapd_ready = ctx.snapd();
    let flatpak_ready = ctx.flatpak();
    let yubico_ready = ctx.yubico();
    let nix_ready = ctx.nix();

    let mut package_names: Vec<&str> = vec!["linux-tools-common", "uidmap"];
    if ctx.env.architecture() == "x86_64" {
        package_names.extend(["syslinux", "libsqlite3-dev", "sqlite3"]);
    }

    let pkg_ids: Vec<_> = package_names
        .iter()
        .map(|name| {
            ctx.plan.add(AptPackage {
                name: (*name).to_string(),
                deps: vec![apt_ready],
                ..Default::default()
            })
        })
        .collect();

    let mut all = pkg_ids;
    all.extend([
        pip_ready,
        snapd_ready,
        flatpak_ready,
        yubico_ready,
        nix_ready,
    ]);

    ctx.plan.add(Marker {
        name: "common_tools:ready".to_string(),
        deps: all,
        ..Default::default()
    })
}
