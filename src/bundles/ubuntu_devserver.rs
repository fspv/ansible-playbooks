use crate::backends::marker::Marker;
use crate::resource::ResourceId;

use super::Context;

// Mirrors roles/ubuntu-devserver. Pulls in docker + libvirtd + et + nvim
// + node_exporter + smartctl_exporter (matches the legacy role's meta).

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let docker_ready = ctx.docker();
    let libvirtd_ready = ctx.libvirtd();
    let et_ready = ctx.et();
    let nvim_ready = ctx.nvim();
    let node_exporter_ready = ctx.node_exporter();
    let smartctl_exporter_ready = ctx.smartctl_exporter();

    ctx.plan.add(Marker {
        name: "ubuntu_devserver:ready".to_string(),
        deps: vec![
            docker_ready,
            libvirtd_ready,
            et_ready,
            nvim_ready,
            node_exporter_ready,
            smartctl_exporter_ready,
        ],
        ..Default::default()
    })
}
