use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/systemd. Configures journald to keep logs in memory only
// (Storage=volatile) and cap journal size at 10 MiB. Restarts
// systemd-journald when the file changes.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let conf = ctx.plan.add(File {
        path: PathBuf::from("/etc/systemd/journald.conf"),
        content: "[Journal]\n\
                  # In-memory only, don't really need older logs\n\
                  Storage=volatile\n\
                  SystemMaxUse=10M\n"
            .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    // started: true so the Service backend's restart_on check fires on
    // config drift — without it `needs_restart` is gated off.
    let service = ctx.plan.add(Service {
        name: "systemd-journald.service".to_string(),
        started: true,
        restart_on: vec![conf],
        deps: vec![conf],
        skip_when: Skip::InContainer,
        ..Default::default()
    });

    ctx.plan.add(Marker {
        name: "systemd:ready".to_string(),
        deps: vec![conf, service],
        ..Default::default()
    })
}
