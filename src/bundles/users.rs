use std::path::PathBuf;

use crate::backends::marker::Marker;
use crate::backends::user::User;
use crate::resource::ResourceId;

use super::Context;

// Default supplementary groups derived from roles/user/defaults/main.yml.
// `lpadmin` (CUPS printer admin) was dropped — host-setup runs on servers,
// not desktops. `nix-users` is left in: hosts that don't install nix should
// remove it from their per-host config (useradd will otherwise fail loudly).

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    // Snapshot the spec list so the borrow on `ctx.config` is released
    // before we start mutating `ctx.plan`.
    let users: Vec<(String, crate::config::UserSpec)> = ctx
        .config
        .users
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let mut all_users = Vec::new();
    for (name, spec) in &users {
        let mut groups: Vec<String> = [
            "audio",
            "cdrom",
            "dip",
            "plugdev",
            "video",
            "disk",
            "nix-users",
            "dialout",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect();

        if spec.admin {
            groups.push("sudo".to_string());
        }
        groups.extend(spec.groups.iter().cloned());

        let id = ctx.plan.add(User {
            name: name.clone(),
            uid: spec.uid,
            comment: spec.comment.clone(),
            home: spec.home.clone(),
            shell: spec
                .shell
                .clone()
                .or_else(|| Some(PathBuf::from("/bin/bash"))),
            groups,
            password_hash: spec.password.clone(),
            create_home: true,
            ..Default::default()
        });
        all_users.push(id);
    }

    ctx.plan.add(Marker {
        name: "users:ready".to_string(),
        deps: all_users,
        ..Default::default()
    })
}
