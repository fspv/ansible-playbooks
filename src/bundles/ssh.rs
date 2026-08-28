use crate::backends::absent_file::AbsentFile;
use crate::backends::command::Command;
use crate::backends::marker::Marker;
use crate::resource::{ResourceId, Skip};

use super::Context;

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let removed_drop_ins = remove_every_sshd_config_drop_in(ctx);

    let sshd_restart = ctx.plan.add(Command {
        name: "systemctl restart ssh".to_string(),
        argv: vec![
            "systemctl".to_string(),
            "restart".to_string(),
            "ssh".to_string(),
        ],
        trigger_on: Some(removed_drop_ins.clone()),
        deps: removed_drop_ins.clone(),
        skip_when: Skip::InContainer,
    });

    let mut deps = removed_drop_ins;
    deps.push(sshd_restart);

    ctx.plan.add(Marker {
        name: "ssh:ready".to_string(),
        deps,
        ..Default::default()
    })
}

fn remove_every_sshd_config_drop_in(ctx: &mut Context<'_>) -> Vec<ResourceId> {
    let Ok(drop_ins) = std::fs::read_dir("/etc/ssh/sshd_config.d") else {
        return Vec::new();
    };

    drop_ins
        .flatten()
        .map(|drop_in| {
            ctx.plan.add(AbsentFile {
                path: drop_in.path(),
                ..Default::default()
            })
        })
        .collect()
}
