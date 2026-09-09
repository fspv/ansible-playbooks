use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::absent_file::AbsentFile;
use crate::backends::apt_package::AptPackage;
use crate::backends::apt_repo::AptRepo;
use crate::backends::download::Download;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/tailscale/. The keyring is shared across releases (tailscale
// signs all suites with the same key), so the key URL stays pinned to the
// jammy path regardless of the host's codename.

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

    let legacy_key = ctx.plan.add(AbsentFile {
        path: PathBuf::from("/usr/share/keyrings/tailscale-archive-keyring.gpg"),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let key = ctx.plan.add(Download {
        url: "https://pkgs.tailscale.com/stable/ubuntu/jammy.asc".to_string(),
        path: PathBuf::from("/etc/apt/keyrings/tailscale.asc"),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let repo = ctx.plan.add(AptRepo {
        name: "tailscale".to_string(),
        list_content: format!(
            "deb [signed-by=/etc/apt/keyrings/tailscale.asc] \
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
        deps: vec![pin, legacy_key, key, repo, pkg],
        ..Default::default()
    })
}
