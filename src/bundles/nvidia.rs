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

// Mirrors roles/nvidia/. Gated on `config.nvidia` — when false the marker is
// emitted with no deps so downstream bundles can depend on `nvidia:ready`
// unconditionally.
//
// Skipped vs the legacy role: the post-install handlers
// (`nvidia-ctk runtime configure`, `nvidia-ctk cdi generate`,
// `nvidia-ctk user runtime configure`) wire docker into the GPU runtime and
// therefore belong in the docker bundle (which already has TODOs about
// nvidia). Keeping them out of this bundle avoids a circular ctx.docker() ->
// ctx.nvidia() dependency.

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    if !ctx.config.nvidia {
        return ctx.plan.add(Marker {
            name: "nvidia:ready".to_string(),
            deps: vec![],
            ..Default::default()
        });
    }

    let apt_ready = ctx.apt();

    let pin = ctx.plan.add(File {
        path: PathBuf::from("/etc/apt/preferences.d/nvidia.pref"),
        content: "Package: nvidia-container-toolkit\n\
                  Pin: origin nvidia.github.io\n\
                  Pin-Priority: 995\n"
            .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    // The legacy template references `$(ARCH)` — an apt-time substitution
    // performed by apt itself, not by ansible — so it stays literal in the
    // .list body.
    let key = ctx.plan.add(Command {
        name: "fetch nvidia-container-toolkit signing key".to_string(),
        argv: vec![
            "gpg".to_string(),
            "--no-default-keyring".to_string(),
            "--keyring".to_string(),
            "/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg".to_string(),
            "--keyserver".to_string(),
            "keyserver.ubuntu.com".to_string(),
            "--recv-keys".to_string(),
            "DDCAE044F796ECB0".to_string(),
        ],
        deps: vec![apt_ready],
        ..Default::default()
    });

    let repo = ctx.plan.add(AptRepo {
        name: "nvidia".to_string(),
        list_content: "deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] \
                       https://nvidia.github.io/libnvidia-container/stable/deb/$(ARCH) /\n\
                       #deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] \
                       https://nvidia.github.io/libnvidia-container/experimental/deb/$(ARCH) /\n"
            .to_string(),
        deps: vec![apt_ready, pin, key],
        ..Default::default()
    });

    let unattended_blacklist = ctx.plan.add(File {
        path: PathBuf::from("/etc/apt/apt.conf.d/51unattended-upgrades-nvidia-blacklist"),
        content: "// Blacklist NVIDIA packages from automatic upgrades\n\
                  // This prevents unattended-upgrades from automatically updating NVIDIA drivers\n\
                  // which could cause compatibility issues with CUDA applications\n\
                  \n\
                  Unattended-Upgrade::Package-Blacklist {\n    \
                      \"nvidia-driver\";\n    \
                      \"nvidia-dkms\";\n    \
                      \"nvidia-kernel\";\n    \
                      \"libnvidia\";\n    \
                      \"nvidia-*\";\n\
                  };"
        .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let toolkit = ctx.plan.add(AptPackage {
        name: "nvidia-container-toolkit".to_string(),
        deps: vec![apt_ready, repo, unattended_blacklist],
        ..Default::default()
    });

    ctx.plan.add(Marker {
        name: "nvidia:ready".to_string(),
        deps: vec![pin, key, repo, unattended_blacklist, toolkit],
        ..Default::default()
    })
}
