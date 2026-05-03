use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::apt_package::AptPackage;
use crate::backends::command::Command;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/apc-ups/. Disabled in containers — udev does not exist there
// and the rule would never be evaluated. Matches the legacy
// `when: ansible_virtualization_type != 'docker'` gate.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    if ctx.env.is_container() {
        return ctx.plan.add(Marker {
            name: "apc_ups:ready".to_string(),
            deps: vec![],
            ..Default::default()
        });
    }

    let apt_ready = ctx.apt();

    let udev_pkg = ctx.plan.add(AptPackage {
        name: "udev".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let rule = ctx.plan.add(File {
        path: PathBuf::from("/etc/udev/rules.d/99-apc-ups.rules"),
        content: "# APC UPS devices\n\
                  SUBSYSTEM==\"usb\", ATTRS{idVendor}==\"051d\", GROUP=\"dialout\", MODE=\"0664\"\n\
                  SUBSYSTEM==\"hidraw\", ATTRS{idVendor}==\"051d\", GROUP=\"dialout\", MODE=\"0664\"\n\
                  KERNEL==\"hiddev*\", ATTRS{idVendor}==\"051d\", GROUP=\"dialout\", MODE=\"0664\"\n"
            .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![udev_pkg],
        ..Default::default()
    });

    let udev_reload = ctx.plan.add(Command {
        name: "udevadm reload-rules + trigger (apc-ups)".to_string(),
        argv: vec![
            "sh".to_string(),
            "-c".to_string(),
            "udevadm control --reload-rules && udevadm trigger".to_string(),
        ],
        trigger_on: Some(vec![rule]),
        deps: vec![rule],
        skip_when: Skip::InContainer,
    });

    ctx.plan.add(Marker {
        name: "apc_ups:ready".to_string(),
        deps: vec![udev_pkg, rule, udev_reload],
        ..Default::default()
    })
}
