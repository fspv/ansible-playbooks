use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::backends::systemd_unit::SystemdUnit;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/node_exporter/. The legacy role gated each task on
// `ansible_virtualization_type != 'docker'` via `ignore_errors`; we replace
// that with `Skip::InContainer` so the bundle is a clean no-op inside
// containers rather than swallowing real failures. The legacy meta declares
// docker as a dependency because the unit shells out to podman; we mirror
// that by pulling `ctx.docker()` even though podman itself isn't installed
// by the docker bundle today — the dep chain is preserved for when it is.
pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let docker_ready = ctx.docker();

    let unit = ctx.plan.add(SystemdUnit {
        name: "node_exporter.service".to_string(),
        content: r"[Unit]
Description=Node Exporter
After=network.target

[Service]
Type=simple
ExecStartPre=-/usr/bin/podman rm -f node_exporter
ExecStart=/usr/bin/podman run --rm \
    --name node_exporter \
    --network host \
    --pid host \
    --volume /:/host:ro,rslave \
    --volume /run/udev:/run/udev:ro \
    quay.io/prometheus/node-exporter:latest \
    --path.rootfs=/host
ExecStop=/usr/bin/podman stop node_exporter
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
"
        .to_string(),
        deps: vec![docker_ready],
        skip_when: Skip::InContainer,
    });

    let service = ctx.plan.add(Service {
        name: "node_exporter.service".to_string(),
        enabled: true,
        started: true,
        restart_on: vec![unit],
        deps: vec![unit],
        skip_when: Skip::InContainer,
    });

    ctx.plan.add(Marker {
        name: "node_exporter:ready".to_string(),
        deps: vec![unit, service],
        skip_when: Skip::InContainer,
    })
}
