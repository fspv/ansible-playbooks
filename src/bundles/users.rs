use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::backends::user::User;
use crate::config::UserSpec;
use crate::env::Env;
use crate::plan::Plan;

// Default supplementary groups derived from roles/user/defaults/main.yml.
// `lpadmin` (CUPS printer admin) was dropped — host-setup runs on servers,
// not desktops. `nix-users` is left in: hosts that don't install nix should
// remove it from their per-host config (useradd will otherwise fail loudly).

pub fn apply(plan: &mut Plan, _env: &Env, users: &BTreeMap<String, UserSpec>) {
    for (name, spec) in users {
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

        plan.add(User {
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
    }
}
