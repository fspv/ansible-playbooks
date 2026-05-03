use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::apt_package::AptPackage;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::backends::symlink::Symlink;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/tzdata/. The timezone is hardcoded to Europe/London — the
// only value the legacy role's defaults set. When per-host tz becomes
// configurable, plumb it through `Config` and read it here.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let pkg = ctx.plan.add(AptPackage {
        name: "tzdata".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let localtime = ctx.plan.add(Symlink {
        path: PathBuf::from("/etc/localtime"),
        target: PathBuf::from("/usr/share/zoneinfo/Europe/London"),
        deps: vec![pkg],
        ..Default::default()
    });

    let timezone = ctx.plan.add(File {
        path: PathBuf::from("/etc/timezone"),
        content: "Europe/London\n".to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![pkg],
        ..Default::default()
    });

    ctx.plan.add(Marker {
        name: "tzdata:ready".to_string(),
        deps: vec![pkg, localtime, timezone],
        ..Default::default()
    })
}
