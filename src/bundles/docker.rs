use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::absent_apt_package::AbsentAptPackage;
use crate::backends::apt_package::AptPackage;
use crate::backends::apt_repo::AptRepo;
use crate::backends::directory::Directory;
use crate::backends::download::Download;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::backends::systemd_unit::SystemdUnit;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/docker/. Differences from the ansible role:
//  * The repo file lives at /etc/apt/sources.list.d/docker.list (the AptRepo
//    backend's modern convention) rather than docker-ce.list, and the key is
//    in /etc/apt/keyrings/docker.asc with `signed-by=` instead of being
//    dropped under /etc/apt/trusted.gpg.d/.
//  * The signing key is fetched at converge time from
//    https://download.docker.com/linux/ubuntu/gpg via the `Download`
//    backend — no embedded copy in the source. Docker's key rotates rarely
//    but if/when they do, the next converge picks the new key up
//    automatically.
//  * apt preferences (docker-ce.pref) are not pinned — the upstream repo is
//    the only source of these packages on Ubuntu noble, so a pin is moot.
//  * The architecture string is hard-coded to amd64 to keep the bundle data-
//    only; arm64 hosts will need a small generalisation when added.
//  * Daemon config substitutes the `nvidia` runtime block out: this bundle
//    doesn't know whether the host has a GPU, so the runtimes map is left
//    empty. The nvidia role can layer its own /etc/docker/daemon.json on
//    top later.
//  * Skipped: podman packages, removal of unused packages, nvidia-* config
//    services (no GPU on the framework's target hosts), and the per-user
//    run-args list. None of these are expressible with the current backend
//    set or are out of scope.

// Body length is data, not logic — most of it is verbatim systemd unit
// text inlined per project convention.
#[allow(clippy::too_many_lines)]
pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();

    let key = ctx.plan.add(Download {
        url: "https://download.docker.com/linux/ubuntu/gpg".to_string(),
        path: PathBuf::from("/etc/apt/keyrings/docker.asc"),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let docker_repo = ctx.plan.add(AptRepo {
        name: "docker".to_string(),
        list_content: "deb [arch=amd64 signed-by=/etc/apt/keyrings/docker.asc] \
             https://download.docker.com/linux/ubuntu noble stable\n"
            .to_string(),
        deps: vec![apt_ready, key],
        ..Default::default()
    });

    let package_ids: Vec<_> = [
        "docker-ce",
        "docker-ce-cli",
        "containerd.io",
        "docker-buildx-plugin",
        "docker-compose-plugin",
    ]
    .iter()
    .map(|name| {
        ctx.plan.add(AptPackage {
            name: (*name).to_string(),
            deps: vec![docker_repo],
            ..Default::default()
        })
    })
    .collect();

    let daemon_json = ctx.plan.add(File {
        path: PathBuf::from("/etc/docker/daemon.json"),
        content: r#"{
  "iptables": false,
  "ipv6": true,
  "fixed-cidr-v6": "5051::/112",
  "metrics-addr": "0.0.0.0:9323",
  "runtimes": {
  }
}
"#
        .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: package_ids.clone(),
        ..Default::default()
    });

    // The legacy template substitutes {{ docker_service_systemd_custom_opts }}
    // (defaulting to "--experimental=true" in roles/docker/defaults/main.yml).
    // Resolved inline; if a host needs different flags, layer its own drop-in.
    let docker_dropin = ctx.plan.add(SystemdUnit {
        name: "docker.service.d/custom-docker-opts.conf".to_string(),
        content: "[Service]\n\
                  ExecStart=\n\
                  ExecStart=/usr/bin/dockerd -H unix:// --experimental=true\n"
            .to_string(),
        deps: vec![package_ids[0]],
        ..Default::default()
    });

    let docker_service = ctx.plan.add(Service {
        name: "docker.service".to_string(),
        enabled: true,
        started: true,
        restart_on: vec![daemon_json, docker_dropin],
        deps: vec![daemon_json, docker_dropin],
        skip_when: Skip::InContainer,
    });

    let docker_cleanup_service = ctx.plan.add(SystemdUnit {
        name: "docker-cleanup.service".to_string(),
        content: "[Unit]\n\
                  Description=Docker System Cleanup\n\
                  ConditionACPower=true\n\
                  \n\
                  [Service]\n\
                  Type=oneshot\n\
                  ExecStart=/usr/bin/docker system prune -a --volumes -f\n"
            .to_string(),
        deps: package_ids.clone(),
        ..Default::default()
    });

    let docker_cleanup_timer_unit = ctx.plan.add(SystemdUnit {
        name: "docker-cleanup.timer".to_string(),
        content: "[Unit]\n\
                  Description=Daily Docker System Cleanup Timer\n\
                  Requires=docker-cleanup.service\n\
                  \n\
                  [Timer]\n\
                  OnCalendar=daily\n\
                  Persistent=true\n\
                  \n\
                  [Install]\n\
                  WantedBy=timers.target\n"
            .to_string(),
        deps: vec![docker_cleanup_service],
        ..Default::default()
    });

    let docker_cleanup_timer = ctx.plan.add(Service {
        name: "docker-cleanup.timer".to_string(),
        enabled: true,
        started: true,
        deps: vec![docker_cleanup_timer_unit],
        skip_when: Skip::InContainer,
        ..Default::default()
    });

    let podman_cleanup_service = ctx.plan.add(SystemdUnit {
        name: "podman-cleanup.service".to_string(),
        content: "[Unit]\n\
                  Description=Podman System Cleanup\n\
                  ConditionACPower=true\n\
                  \n\
                  [Service]\n\
                  Type=oneshot\n\
                  ExecStart=/usr/bin/podman system prune -a --volumes -f\n"
            .to_string(),
        deps: package_ids.clone(),
        ..Default::default()
    });

    let podman_cleanup_timer_unit = ctx.plan.add(SystemdUnit {
        name: "podman-cleanup.timer".to_string(),
        content: "[Unit]\n\
                  Description=Daily Podman System Cleanup Timer\n\
                  Requires=podman-cleanup.service\n\
                  \n\
                  [Timer]\n\
                  OnCalendar=daily\n\
                  Persistent=true\n\
                  \n\
                  [Install]\n\
                  WantedBy=timers.target\n"
            .to_string(),
        deps: vec![podman_cleanup_service],
        ..Default::default()
    });

    let podman_cleanup_timer = ctx.plan.add(Service {
        name: "podman-cleanup.timer".to_string(),
        enabled: true,
        started: true,
        deps: vec![podman_cleanup_timer_unit],
        skip_when: Skip::InContainer,
        ..Default::default()
    });

    let removed_ids: Vec<_> = [
        "docker.io",
        "docker-compose",
        "docker-compose-v2",
        "docker-doc",
    ]
    .iter()
    .map(|name| {
        ctx.plan.add(AbsentAptPackage {
            name: (*name).to_string(),
            deps: vec![apt_ready],
            ..Default::default()
        })
    })
    .collect();

    let users_ready = ctx.users();
    // Snapshot users so the borrow on ctx.config releases before we mutate plan.
    let user_specs: Vec<(String, crate::config::UserSpec)> = ctx
        .config
        .users
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let mut per_user_ids: Vec<ResourceId> = Vec::new();
    for (name, spec) in &user_specs {
        let Some(home) = spec.home.as_ref() else {
            continue;
        };
        let dir_id = ctx.plan.add(Directory {
            path: PathBuf::from(format!("{}/.config/docker-user", home.display())),
            mode: Some(Permissions::from_mode(0o700)),
            owner: Some(name.clone()),
            deps: vec![package_ids[0], users_ready],
            ..Default::default()
        });
        // TODO: File backend has no `owner` field yet — run-args ends up
        // root-owned. The legacy role chowned to the user. Plumb owner
        // through File (or add a chown backend) and apply here.
        let run_args_id = ctx.plan.add(File {
            path: PathBuf::from(format!("{}/.config/docker-user/run-args", home.display())),
            content: "--mount=\"type=bind,source=${HOME}/.config/docker-user/ansible-playbooks-work,destination=/mnt/my/ansible-work\" \
                      --mount=\"type=bind,source=${HOME}/.config/docker-user/ansible-playbooks,destination=/mnt/my/ansible\" \
                      --mount=\"type=bind,source=${HOME}/.gnupg/,destination=/root/.gnupg/\"\n"
                .to_string(),
            mode: Some(Permissions::from_mode(0o600)),
            deps: vec![dir_id],
            ..Default::default()
        });
        per_user_ids.push(dir_id);
        per_user_ids.push(run_args_id);
    }

    let mut all = vec![key, docker_repo];
    all.extend(package_ids);
    all.extend([
        daemon_json,
        docker_dropin,
        docker_service,
        docker_cleanup_service,
        docker_cleanup_timer_unit,
        docker_cleanup_timer,
        podman_cleanup_service,
        podman_cleanup_timer_unit,
        podman_cleanup_timer,
    ]);
    all.extend(removed_ids);
    all.extend(per_user_ids);

    ctx.plan.add(Marker {
        name: "docker:ready".to_string(),
        deps: all,
        ..Default::default()
    })
}
