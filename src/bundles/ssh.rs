use crate::backends::absent_file::AbsentFile;
use crate::backends::command::Command;
use crate::backends::marker::Marker;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/ssh. Ubuntu ships drop-ins under /etc/ssh/sshd_config.d
// (cloud-init, vendor packages) that override /etc/ssh/sshd_config, so the
// settings in the main config never take effect. The legacy role used
// `with_fileglob: /etc/ssh/sshd_config.d/*`; reproduce that by enumerating
// the directory at bundle build time and emitting one AbsentFile per entry,
// same as common_tweaks does for the vte profile.d shims. A missing
// directory (openssh-server not installed) yields nothing, and with nothing
// removed the restart never triggers.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let mut drop_in_absents: Vec<ResourceId> = Vec::new();
    if let Ok(entries) = std::fs::read_dir("/etc/ssh/sshd_config.d") {
        for entry in entries.flatten() {
            drop_in_absents.push(ctx.plan.add(AbsentFile {
                path: entry.path(),
                ..Default::default()
            }));
        }
    }

    let sshd_restart = ctx.plan.add(Command {
        name: "systemctl restart ssh".to_string(),
        argv: vec![
            "systemctl".to_string(),
            "restart".to_string(),
            "ssh".to_string(),
        ],
        trigger_on: Some(drop_in_absents.clone()),
        deps: drop_in_absents.clone(),
        skip_when: Skip::InContainer,
    });

    let mut deps = drop_in_absents;
    deps.push(sshd_restart);

    ctx.plan.add(Marker {
        name: "ssh:ready".to_string(),
        deps,
        ..Default::default()
    })
}
