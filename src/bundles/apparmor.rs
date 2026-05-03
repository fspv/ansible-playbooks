use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::apt_package::AptPackage;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/apparmor. The legacy role's `ignore_errors` for the service
// restart in docker virtualization is replaced by `Skip::InContainer` —
// inside a container apparmor isn't manageable, so the resource is skipped
// rather than failing. Mode 0666 on the bwrap profile matches the legacy
// task verbatim; world-writable is unusual for an apparmor profile and is
// almost certainly a typo in the original role, but path-compat takes
// priority over fixing it here.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let pkg = ctx.plan.add(AptPackage {
        name: "apparmor".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let bwrap_profile = ctx.plan.add(File {
        path: PathBuf::from("/etc/apparmor.d/bwrap"),
        content: "abi <abi/4.0>,\n\
                  include <tunables/global>\n\
                  \n\
                  profile bwrap /nix/store/*/bin/bwrap flags=(unconfined) {\n  \
                    userns,\n\
                  \n  \
                    # Site-specific additions and overrides. See local/README for details.\n  \
                    include if exists <local/bwrap>\n\
                  }\n"
        .to_string(),
        mode: Some(Permissions::from_mode(0o666)),
        deps: vec![pkg],
        ..Default::default()
    });

    let service = ctx.plan.add(Service {
        name: "apparmor.service".to_string(),
        enabled: true,
        started: true,
        deps: vec![pkg, bwrap_profile],
        skip_when: Skip::InContainer,
    });

    ctx.plan.add(Marker {
        name: "apparmor:ready".to_string(),
        deps: vec![pkg, bwrap_profile, service],
        ..Default::default()
    })
}
