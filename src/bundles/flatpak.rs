use crate::backends::apt_package::AptPackage;
use crate::backends::command::Command;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let pkg = ctx.plan.add(AptPackage {
        name: "flatpak".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let flathub = ctx.plan.add(Command {
        name: "flatpak remote-add flathub".to_string(),
        argv: vec![
            "flatpak".to_string(),
            "remote-add".to_string(),
            "--if-not-exists".to_string(),
            "flathub".to_string(),
            "https://dl.flathub.org/repo/flathub.flatpakrepo".to_string(),
        ],
        deps: vec![pkg],
        ..Default::default()
    });

    ctx.plan.add(Marker {
        name: "flatpak:ready".to_string(),
        deps: vec![pkg, flathub],
        ..Default::default()
    })
}
