use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::apt_package::AptPackage;
use crate::backends::apt_repo::AptRepo;
use crate::backends::command::Command;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/tailscale/. The legacy `apt_key` task fetched
// https://pkgs.tailscale.com/stable/ubuntu/jammy.noarmor.gpg into
// /usr/share/keyrings/tailscale-archive-keyring.gpg; we replicate that with
// curl + tee. The keyring is shared across releases (tailscale signs all
// suites with the same key), so the URL stays pinned to `jammy.noarmor.gpg`
// regardless of the host's codename.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();
    let codename = ctx.env.ubuntu_codename();

    let pin = ctx.plan.add(File {
        path: PathBuf::from("/etc/apt/preferences.d/tailscale.pref"),
        content: "Package: tailscale\n\
                  Pin: origin pkgs.tailscale.com\n\
                  Pin-Priority: 995\n"
            .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let key = ctx.plan.add(Command {
        name: "fetch tailscale signing key".to_string(),
        argv: vec![
            "sh".to_string(),
            "-c".to_string(),
            "curl -fsSL https://pkgs.tailscale.com/stable/ubuntu/jammy.noarmor.gpg \
             -o /usr/share/keyrings/tailscale-archive-keyring.gpg"
                .to_string(),
        ],
        deps: vec![apt_ready],
        ..Default::default()
    });

    let repo = ctx.plan.add(AptRepo {
        name: "tailscale".to_string(),
        list_content: format!(
            "deb [signed-by=/usr/share/keyrings/tailscale-archive-keyring.gpg] \
             https://pkgs.tailscale.com/stable/ubuntu {codename} main\n",
        ),
        deps: vec![apt_ready, pin, key],
        ..Default::default()
    });

    let pkg = ctx.plan.add(AptPackage {
        name: "tailscale".to_string(),
        deps: vec![apt_ready, repo],
        ..Default::default()
    });

    ctx.plan.add(Marker {
        name: "tailscale:ready".to_string(),
        deps: vec![pin, key, repo, pkg],
        ..Default::default()
    })
}
