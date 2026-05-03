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

// Mirrors roles/docker/. Differences from the legacy ansible role:
//  * Repo file lives at /etc/apt/sources.list.d/docker.list (the AptRepo
//    backend's modern convention) rather than docker-ce.list, and the key
//    is in /etc/apt/keyrings/docker.asc with `signed-by=` instead of being
//    dropped under /etc/apt/trusted.gpg.d/.
//  * `/etc/systemd/user/nvidia-ctk-docker-config.service` is written when
//    nvidia is enabled. Per-user `systemctl --user enable` is still out of
//    scope, so the unit lands but isn't enabled — matching the orphan
//    template in the legacy ansible role (the file existed, no task ever
//    enabled it).

// Body length is data, not logic — most of it is verbatim systemd unit
// text inlined per project convention.
#[allow(clippy::too_many_lines)]
pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();
    let users_ready = ctx.users();
    let nvidia_ready = ctx.nvidia();
    let nvidia_enabled = ctx.config.nvidia;
    let apt_arch = ctx.env.apt_arch();
    let codename = ctx.env.ubuntu_codename();

    let key = ctx.plan.add(Download {
        url: "https://download.docker.com/linux/ubuntu/gpg".to_string(),
        path: PathBuf::from("/etc/apt/keyrings/docker.asc"),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let pin = ctx.plan.add(File {
        path: PathBuf::from("/etc/apt/preferences.d/docker-ce.pref"),
        content: "Package: *\n\
                  Pin: origin download.docker.com\n\
                  Pin-Priority: 995\n"
            .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let docker_repo = ctx.plan.add(AptRepo {
        name: "docker".to_string(),
        list_content: format!(
            "deb [arch={apt_arch} signed-by=/etc/apt/keyrings/docker.asc] \
             https://download.docker.com/linux/ubuntu {codename} stable\n"
        ),
        deps: vec![apt_ready, pin, key],
        ..Default::default()
    });

    let package_ids: Vec<_> = [
        "docker-ce",
        "docker-ce-cli",
        "containerd.io",
        "docker-buildx-plugin",
        "docker-compose-plugin",
        "podman",
        "podman-compose",
        "crun",
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

    // The legacy template emits an `nvidia` runtime entry inside
    // `runtimes` only when the host is a GPU box. Match that exactly so
    // dpkg's bytewise compare lines up after either tool runs.
    let runtimes_block = if nvidia_enabled {
        "    \"nvidia\": {\n      \"args\": [],\n      \"path\": \"nvidia-container-runtime\"\n    }\n  "
    } else {
        ""
    };
    let daemon_json = ctx.plan.add(File {
        path: PathBuf::from("/etc/docker/daemon.json"),
        content: format!(
            "{{\n  \"iptables\": false,\n  \"ipv6\": true,\n  \"fixed-cidr-v6\": \"5051::/112\",\n  \"metrics-addr\": \"0.0.0.0:9323\",\n  \"runtimes\": {{\n  {runtimes_block}}}\n}}\n"
        ),
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

    // Nvidia oneshot units that replace the legacy nvidia handlers
    // (`nvidia-ctk runtime configure`, `nvidia-ctk cdi generate`). Each is
    // defined unconditionally — wiring the corresponding Service in only
    // happens when ctx.config.nvidia is true. The units are no-ops on
    // non-GPU hosts because of the ConditionPathExists guard.
    let nvidia_cdi_generate_unit = ctx.plan.add(SystemdUnit {
        name: "nvidia-cdi-generate.service".to_string(),
        content: "[Unit]\n\
                  Description=Generate NVIDIA CDI specification\n\
                  ConditionPathExists=/usr/bin/nvidia-ctk\n\
                  Before=docker.service\n\
                  Before=containerd.service\n\
                  \n\
                  [Service]\n\
                  Type=oneshot\n\
                  RemainAfterExit=yes\n\
                  ExecStartPre=/bin/mkdir -p /etc/cdi\n\
                  ExecStart=/usr/bin/nvidia-ctk cdi generate --output=/etc/cdi/nvidia.yaml\n\
                  \n\
                  [Install]\n\
                  WantedBy=multi-user.target\n"
            .to_string(),
        deps: package_ids.clone(),
        ..Default::default()
    });
    let nvidia_ctk_containerd_unit = ctx.plan.add(SystemdUnit {
        name: "nvidia-ctk-containerd-config.service".to_string(),
        content: "[Unit]\n\
                  Description=Configure NVIDIA Container Toolkit for containerd\n\
                  ConditionPathExists=/usr/bin/nvidia-ctk\n\
                  Before=containerd.service\n\
                  \n\
                  [Service]\n\
                  Type=oneshot\n\
                  RemainAfterExit=yes\n\
                  ExecStart=/usr/bin/nvidia-ctk runtime configure --runtime=containerd\n\
                  \n\
                  [Install]\n\
                  WantedBy=multi-user.target\n"
            .to_string(),
        deps: package_ids.clone(),
        ..Default::default()
    });
    let nvidia_ctk_crio_unit = ctx.plan.add(SystemdUnit {
        name: "nvidia-ctk-crio-config.service".to_string(),
        content: "[Unit]\n\
                  Description=Configure NVIDIA Container Toolkit for CRI-O\n\
                  ConditionPathExists=/usr/bin/nvidia-ctk\n\
                  Before=crio.service\n\
                  \n\
                  [Service]\n\
                  Type=oneshot\n\
                  RemainAfterExit=yes\n\
                  ExecStart=/usr/bin/nvidia-ctk runtime configure --runtime=crio\n\
                  \n\
                  [Install]\n\
                  WantedBy=multi-user.target\n"
            .to_string(),
        deps: package_ids.clone(),
        ..Default::default()
    });

    // Restart docker when the nvidia toolkit changes — the legacy "docker
    // restart" handler from roles/nvidia/tasks/packages.yml. nvidia_ready
    // is a Marker covering the whole nvidia bundle (toolkit pkg, repo,
    // pins, etc.), so any change there bumps docker.
    let mut docker_restart_on = vec![daemon_json, docker_dropin];
    if nvidia_enabled {
        docker_restart_on.push(nvidia_ready);
    }
    let mut docker_deps = vec![daemon_json, docker_dropin, nvidia_ready];
    if nvidia_enabled {
        docker_deps.push(nvidia_cdi_generate_unit);
        docker_deps.push(nvidia_ctk_containerd_unit);
    }
    let docker_service = ctx.plan.add(Service {
        name: "docker.service".to_string(),
        enabled: true,
        started: true,
        restart_on: docker_restart_on,
        deps: docker_deps,
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

    let nvidia_user_unit_id: Option<ResourceId> = if nvidia_enabled {
        Some(ctx.plan.add(File {
            path: PathBuf::from("/etc/systemd/user/nvidia-ctk-docker-config.service"),
            content: "[Unit]\n\
                      Description=Configure NVIDIA Container Toolkit for Docker\n\
                      ConditionPathExists=/usr/bin/nvidia-ctk\n\
                      Before=podman.socket\n\
                      \n\
                      [Service]\n\
                      Type=oneshot\n\
                      RemainAfterExit=yes\n\
                      ExecStart=/usr/bin/nvidia-ctk runtime configure --runtime=docker --config=%h/.config/docker/daemon.json\n\
                      \n\
                      [Install]\n\
                      WantedBy=default.target\n"
                .to_string(),
            mode: Some(Permissions::from_mode(0o644)),
            deps: package_ids.clone(),
            ..Default::default()
        }))
    } else {
        None
    };

    // Nvidia oneshots: enable+start so they run on boot and re-run when
    // the unit content changes (covers the ansible "service ... + flush
    // handler" pattern).
    let nvidia_service_ids: Vec<ResourceId> = if nvidia_enabled {
        vec![
            ctx.plan.add(Service {
                name: "nvidia-cdi-generate.service".to_string(),
                enabled: true,
                started: true,
                restart_on: vec![nvidia_cdi_generate_unit, nvidia_ready],
                deps: vec![nvidia_cdi_generate_unit, nvidia_ready],
                skip_when: Skip::InContainer,
            }),
            ctx.plan.add(Service {
                name: "nvidia-ctk-containerd-config.service".to_string(),
                enabled: true,
                started: true,
                restart_on: vec![nvidia_ctk_containerd_unit, nvidia_ready],
                deps: vec![nvidia_ctk_containerd_unit, nvidia_ready],
                skip_when: Skip::InContainer,
            }),
            ctx.plan.add(Service {
                name: "nvidia-ctk-crio-config.service".to_string(),
                enabled: true,
                started: true,
                restart_on: vec![nvidia_ctk_crio_unit, nvidia_ready],
                deps: vec![nvidia_ctk_crio_unit, nvidia_ready],
                skip_when: Skip::InContainer,
            }),
        ]
    } else {
        Vec::new()
    };

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
        let run_args_id = ctx.plan.add(File {
            path: PathBuf::from(format!("{}/.config/docker-user/run-args", home.display())),
            content: "--mount=\"type=bind,source=${HOME}/.config/docker-user/ansible-playbooks-work,destination=/mnt/my/ansible-work\" \
                      --mount=\"type=bind,source=${HOME}/.config/docker-user/ansible-playbooks,destination=/mnt/my/ansible\" \
                      --mount=\"type=bind,source=${HOME}/.gnupg/,destination=/root/.gnupg/\"\n"
                .to_string(),
            mode: Some(Permissions::from_mode(0o600)),
            owner: Some(name.clone()),
            deps: vec![dir_id],
            ..Default::default()
        });
        per_user_ids.push(dir_id);
        per_user_ids.push(run_args_id);
    }

    let mut all = vec![key, pin, docker_repo];
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
        nvidia_cdi_generate_unit,
        nvidia_ctk_containerd_unit,
        nvidia_ctk_crio_unit,
    ]);
    all.extend(nvidia_service_ids);
    all.extend(nvidia_user_unit_id);
    all.extend(removed_ids);
    all.extend(per_user_ids);

    ctx.plan.add(Marker {
        name: "docker:ready".to_string(),
        deps: all,
        ..Default::default()
    })
}
