use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use tracing::warn;

use crate::backends::directory::Directory;
use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::backends::systemd_unit::SystemdUnit;
use crate::backends::user::User;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Default supplementary groups derived from roles/user/defaults/main.yml.
// `lpadmin` (CUPS printer admin) was dropped — host-setup runs on servers,
// not desktops. `nix-users` is left in: hosts that don't install nix should
// remove it from their per-host config (useradd will otherwise fail loudly).
//
// Per user, we also drop a private-data dir at ~/.local/private and a
// systemd .mount unit that mounts a tmpfs at ~/.cache (matches the legacy
// `roles/user/tasks/configs.yml`). The custom-secrets handling from the
// legacy role is intentionally out of scope here.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    // Snapshot the spec list so the borrow on `ctx.config` is released
    // before we start mutating `ctx.plan`.
    let users: Vec<(String, crate::config::UserSpec)> = ctx
        .config
        .users
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let mut all_resources: Vec<ResourceId> = Vec::new();
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

        let user_id = ctx.plan.add(User {
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
        all_resources.push(user_id);

        let Some(home) = spec.home.as_ref() else {
            continue;
        };

        let private_dir = ctx.plan.add(Directory {
            path: PathBuf::from(format!("{}/.local/private", home.display())),
            mode: Some(Permissions::from_mode(0o700)),
            owner: Some(name.clone()),
            deps: vec![user_id],
            ..Default::default()
        });
        all_resources.push(private_dir);

        // Legacy gates the .cache mount on `item.user != 'root'` and on the
        // host not being a docker container. The unit name
        // `home-<user>-.cache.mount` is what systemd derives from the path
        // /home/<user>/.cache; if `home` differs from /home/<user>, the unit
        // name doesn't match and systemd would refuse to load the mount.
        // Fail-loudly: log a TODO and skip the .cache mount for that user.
        if name == "root" {
            continue;
        }
        let expected_home = PathBuf::from(format!("/home/{name}"));
        if home.as_path() != Path::new(&expected_home) {
            warn!(
                user = %name,
                home = %home.display(),
                "TODO: .cache tmpfs mount unit name assumes /home/<user>; non-standard home — skipping"
            );
            continue;
        }

        let unit_name = format!("home-{name}-.cache.mount");
        let unit_id = ctx.plan.add(SystemdUnit {
            name: unit_name.clone(),
            content: format!(
                "[Unit]\n\
                 Description=Mount tmpfs to /home/{name}/.cache\n\
                 \n\
                 [Mount]\n\
                 Where=/home/{name}/.cache\n\
                 Options=defaults,noatime,nodiratime,nosuid,nodev,mode=1777\n\
                 What=tmpfs\n\
                 Type=tmpfs\n\
                 \n\
                 [Install]\n\
                 WantedBy=multi-user.target\n"
            ),
            deps: vec![user_id],
            skip_when: Skip::InContainer,
        });
        let svc_id = ctx.plan.add(Service {
            name: unit_name,
            enabled: true,
            started: true,
            deps: vec![unit_id],
            skip_when: Skip::InContainer,
            ..Default::default()
        });
        all_resources.push(unit_id);
        all_resources.push(svc_id);
    }

    ctx.plan.add(Marker {
        name: "users:ready".to_string(),
        deps: all_resources,
        ..Default::default()
    })
}
