use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::apt_package::AptPackage;
use crate::backends::command::Command;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/ca-cert/. Installs ca-certificates, drops per-host PEMs from
// `Config.ca_cert`, and runs update-ca-certificates to (re)build
// /etc/ssl/certs.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let pkg = ctx.plan.add(AptPackage {
        name: "ca-certificates".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let cert_entries: Vec<(String, String)> = ctx
        .config
        .ca_cert
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let cert_ids: Vec<ResourceId> = cert_entries
        .iter()
        .map(|(name, body)| {
            ctx.plan.add(File {
                path: PathBuf::from(format!("/usr/local/share/ca-certificates/{name}.pem")),
                content: body.clone(),
                mode: Some(Permissions::from_mode(0o644)),
                deps: vec![pkg],
                ..Default::default()
            })
        })
        .collect();

    let mut update_deps = vec![pkg];
    update_deps.extend(cert_ids.iter().copied());
    let update = ctx.plan.add(Command {
        name: "update-ca-certificates".to_string(),
        argv: vec!["update-ca-certificates".to_string()],
        deps: update_deps,
        ..Default::default()
    });

    let mut all = vec![pkg];
    all.extend(cert_ids);
    all.push(update);

    ctx.plan.add(Marker {
        name: "ca_cert:ready".to_string(),
        deps: all,
        ..Default::default()
    })
}
