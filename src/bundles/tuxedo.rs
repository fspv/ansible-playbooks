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

// Mirrors roles/tuxedo/. Active only on hosts whose ansible_system_vendor is
// "TUXEDO" — for everyone else the marker is empty so downstream bundles can
// depend on `tuxedo:ready` unconditionally.
//
// Ubuntu codename comes from `ctx.env.ubuntu_codename()` (read from
// /etc/os-release).

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    if ctx.config.system_vendor.as_deref() != Some("TUXEDO") {
        return ctx.plan.add(Marker {
            name: "tuxedo:ready".to_string(),
            deps: vec![],
            ..Default::default()
        });
    }

    let apt_ready = ctx.apt();
    let codename = ctx.env.ubuntu_codename();

    let pins = ctx.plan.add(File {
        path: PathBuf::from("/etc/apt/preferences.d/tuxedo.pref"),
        content: "Package: *\n\
                  Pin: origin mirrors.tuxedocomputers.com\n\
                  Pin-Priority: 1\n\
                  \n\
                  Package: *\n\
                  Pin: origin deb.tuxedocomputers.com\n\
                  Pin-Priority: 1\n\
                  \n\
                  Package: linux-*\n\
                  Pin: origin mirrors.tuxedocomputers.com\n\
                  Pin-Priority: 995\n\
                  \n\
                  Package: tuxedo-*\n\
                  Pin: origin mirrors.tuxedocomputers.com\n\
                  Pin-Priority: 995\n\
                  \n\
                  Package: amd64-microcode\n\
                  Pin: origin mirrors.tuxedocomputers.com\n\
                  Pin-Priority: 995\n\
                  \n\
                  Package: linux-*\n\
                  Pin: origin deb.tuxedocomputers.com\n\
                  Pin-Priority: 995\n\
                  \n\
                  Package: tuxedo-*\n\
                  Pin: origin deb.tuxedocomputers.com\n\
                  Pin-Priority: 995\n\
                  \n\
                  Package: amd64-microcode\n\
                  Pin: origin deb.tuxedocomputers.com\n\
                  Pin-Priority: 995\n"
            .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let key = ctx.plan.add(Command {
        name: "fetch tuxedo signing key".to_string(),
        argv: vec![
            "sh".to_string(),
            "-c".to_string(),
            "curl -fsSL https://deb.tuxedocomputers.com/0x54840598.pub.asc \
             | gpg --dearmor --yes -o /etc/apt/trusted.gpg.d/tuxedo.gpg"
                .to_string(),
        ],
        deps: vec![apt_ready],
        ..Default::default()
    });

    let repo = ctx.plan.add(AptRepo {
        name: "tuxedo-computers".to_string(),
        list_content: format!("deb https://deb.tuxedocomputers.com/ubuntu {codename} main\n"),
        deps: vec![apt_ready, pins, key],
        ..Default::default()
    });

    let package_ids: Vec<_> = [
        "tuxedo-archive-keyring",
        "tuxedo-control-center",
        "tuxedo-keyboard",
        "tuxedo-touchpad-switch",
        "tuxedo-drivers",
        "linux-tuxedo",
    ]
    .iter()
    .map(|name| {
        ctx.plan.add(AptPackage {
            name: (*name).to_string(),
            deps: vec![apt_ready, repo],
            ..Default::default()
        })
    })
    .collect();

    let mut all = vec![pins, key, repo];
    all.extend(package_ids);

    ctx.plan.add(Marker {
        name: "tuxedo:ready".to_string(),
        deps: all,
        ..Default::default()
    })
}
