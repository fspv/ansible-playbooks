use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::apt_package::AptPackage;
use crate::backends::apt_repo::AptRepo;
use crate::backends::command::Command;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/et. The original `apt_key` task fetched
// D3614CB0B3C2D154356BD436CB4ADEA5B72A07A1 from keyserver.ubuntu.com into
// /etc/apt/trusted.gpg.d/ppa-et.gpg; we replicate that with a `gpg --recv-keys`
// Command. `Command` always reports Changed::Yes, so this re-fetches on every
// run — acceptable here, the keyring file content stabilises after the first
// run. The Ubuntu codename in the deb URL comes from
// `ctx.env.ubuntu_codename()`, which reads `/etc/os-release` at startup.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();
    let codename = ctx.env.ubuntu_codename();

    let pin = ctx.plan.add(File {
        path: PathBuf::from("/etc/apt/preferences.d/ppa-et.pref"),
        content: "Package: et\n\
                  Pin: origin ppa.launchpad.net\n\
                  Pin-Priority: 995\n"
            .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let key = ctx.plan.add(Command {
        name: "fetch ppa-et signing key".to_string(),
        argv: vec![
            "gpg".to_string(),
            "--no-default-keyring".to_string(),
            "--keyring".to_string(),
            "/etc/apt/trusted.gpg.d/ppa-et.gpg".to_string(),
            "--keyserver".to_string(),
            "keyserver.ubuntu.com".to_string(),
            "--recv-keys".to_string(),
            "D3614CB0B3C2D154356BD436CB4ADEA5B72A07A1".to_string(),
        ],
        deps: vec![apt_ready],
        ..Default::default()
    });

    let repo = ctx.plan.add(AptRepo {
        name: "ppa-et".to_string(),
        list_content: format!(
            "deb http://ppa.launchpad.net/jgmath2000/et/ubuntu {codename} main\n\
             deb-src http://ppa.launchpad.net/jgmath2000/et/ubuntu {codename} main\n",
        ),
        deps: vec![apt_ready, pin, key],
        ..Default::default()
    });

    let pkg = ctx.plan.add(AptPackage {
        name: "et".to_string(),
        deps: vec![apt_ready, repo],
        ..Default::default()
    });

    let cfg = ctx.plan.add(File {
        path: PathBuf::from("/etc/et.cfg"),
        content: "; et.cfg : Config file for Eternal Terminal\n\
                  ;\n\
                  \n\
                  [Networking]\n\
                  port = 2022\n\
                  # bind_ip = 0.0.0.0\n\
                  \n\
                  [Debug]\n\
                  verbose = 0\n\
                  silent = 0\n\
                  logsize = 20971520\n\
                  telemetry = false\n\
                  logdirectory = /tmp\n"
            .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![pkg],
        ..Default::default()
    });

    let service = ctx.plan.add(Service {
        name: "et.service".to_string(),
        enabled: true,
        started: true,
        restart_on: vec![cfg],
        deps: vec![pkg, cfg],
        skip_when: Skip::InContainer,
    });

    ctx.plan.add(Marker {
        name: "et:ready".to_string(),
        deps: vec![pin, key, repo, pkg, cfg, service],
        ..Default::default()
    })
}
