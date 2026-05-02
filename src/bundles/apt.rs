use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::apt_package::AptPackage;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Paths and bodies are copied verbatim from the legacy ansible role's
// templates under roles/apt/templates/etc/apt/. They must stay byte-exact so
// this bundle can be applied on top of hosts already provisioned by the
// ansible playbook.
//
// Returns the id of an `apt:ready` Marker that depends on every resource
// this bundle added. Downstream bundles call `ctx.apt()` to obtain it.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let confd_files = vec![
        ctx.plan.add(File {
            path: PathBuf::from("/etc/apt/apt.conf.d/00noinstallrecommends"),
            content: "APT::Get::Install-Recommends \"false\";\n\
                      APT::Get::Install-Suggests \"false\";\n"
                .to_string(),
            mode: Some(Permissions::from_mode(0o644)),
            ..Default::default()
        }),
        ctx.plan.add(File {
            path: PathBuf::from("/etc/apt/apt.conf.d/30allowunauth"),
            content: "APT::Get::AllowUnauthenticated \"false\";\n".to_string(),
            mode: Some(Permissions::from_mode(0o644)),
            ..Default::default()
        }),
        ctx.plan.add(File {
            path: PathBuf::from("/etc/apt/apt.conf.d/00timeout"),
            content: "Acquire::http::Timeout \"3\";\n\
                      Acquire::ftp::Timeout \"3\";\n"
                .to_string(),
            mode: Some(Permissions::from_mode(0o644)),
            ..Default::default()
        }),
        ctx.plan.add(File {
            path: PathBuf::from("/etc/apt/apt.conf.d/10periodic"),
            content: "APT::Periodic::Update-Package-Lists \"0\";\n\
                      APT::Periodic::Download-Upgradeable-Packages \"0\";\n\
                      APT::Periodic::AutocleanInterval \"0\";\n\
                      APT::Periodic::Unattended-Upgrade \"0\";\n"
                .to_string(),
            mode: Some(Permissions::from_mode(0o644)),
            ..Default::default()
        }),
    ];

    // Bootstrap packages match roles/apt/tasks/packages.yml verbatim. Several
    // (python3-apt, python3-pycurl, software-properties-common, aptitude) are
    // only relevant when ansible itself drives apt; kept for now so the
    // framework can be applied to ansible-managed hosts without diverging.
    // apt-get update is run by the AptPackage batcher itself before any
    // install, so no explicit Command is needed in this bundle.
    let bootstrap_ids: Vec<_> = [
        "software-properties-common",
        "python3-pycurl",
        "python3-apt",
        "apt-transport-https",
        "openssh-client",
        "aptitude",
    ]
    .iter()
    .map(|name| {
        ctx.plan.add(AptPackage {
            name: (*name).to_string(),
            deps: confd_files.clone(),
            ..Default::default()
        })
    })
    .collect();

    let mut all_apt_resources = confd_files;
    all_apt_resources.extend(bootstrap_ids);

    ctx.plan.add(Marker {
        name: "apt:ready".to_string(),
        deps: all_apt_resources,
        ..Default::default()
    })
}
