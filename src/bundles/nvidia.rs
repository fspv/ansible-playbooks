use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::absent_file::AbsentFile;
use crate::backends::apt_package::AptPackage;
use crate::backends::apt_repo::AptRepo;
use crate::backends::download::Download;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// The nvidia-ctk oneshot units live in the docker bundle: defining them
// here would make ctx.docker() and ctx.nvidia() mutually recursive.

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

    let legacy_key = ctx.plan.add(AbsentFile {
        path: PathBuf::from("/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg"),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let key = ctx.plan.add(Download {
        url: "https://nvidia.github.io/libnvidia-container/gpgkey".to_string(),
        path: PathBuf::from("/etc/apt/keyrings/nvidia-container-toolkit.asc"),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let repo = ctx.plan.add(AptRepo {
        name: "nvidia".to_string(),
        list_content: "deb [signed-by=/etc/apt/keyrings/nvidia-container-toolkit.asc] \
                       https://nvidia.github.io/libnvidia-container/stable/deb/$(ARCH) /\n\
                       #deb [signed-by=/etc/apt/keyrings/nvidia-container-toolkit.asc] \
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
        deps: vec![pin, legacy_key, key, repo, unattended_blacklist, toolkit],
        ..Default::default()
    })
}
