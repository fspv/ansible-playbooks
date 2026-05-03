use std::path::PathBuf;

use crate::backends::apt_package::AptPackage;
use crate::backends::line_in_file::LineInFile;
use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/libvirtd. Installs the qemu/libvirt packages, line-edits
// /etc/libvirt/qemu.conf to disable the apparmor security driver (works
// around the virt-manager "unable to set xattrs" error documented in the
// legacy role's comment), and ensures libvirtd is enabled and running.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let pkg_ids: Vec<_> = [
        "qemu-system",
        "qemu-system-arm",
        "qemu-efi-aarch64",
        "qemu-utils",
        "libvirt-daemon",
        "libvirt-daemon-system",
        "uvtool",
    ]
    .iter()
    .map(|name| {
        ctx.plan.add(AptPackage {
            name: (*name).to_string(),
            deps: vec![apt_ready],
            ..Default::default()
        })
    })
    .collect();

    let security_driver = ctx.plan.add(LineInFile {
        path: PathBuf::from("/etc/libvirt/qemu.conf"),
        regex: r"^\s*#?\s*security_driver\s*=.*".to_string(),
        line: r#"security_driver = "none""#.to_string(),
        deps: pkg_ids.clone(),
        skip_when: Skip::InContainer,
    });

    let service = ctx.plan.add(Service {
        name: "libvirtd.service".to_string(),
        enabled: true,
        started: true,
        deps: {
            let mut d = pkg_ids.clone();
            d.push(security_driver);
            d
        },
        skip_when: Skip::InContainer,
    });

    let mut all = pkg_ids;
    all.push(security_driver);
    all.push(service);

    ctx.plan.add(Marker {
        name: "libvirtd:ready".to_string(),
        deps: all,
        ..Default::default()
    })
}
