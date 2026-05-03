use tracing::warn;

use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::backends::systemd_unit::SystemdUnit;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/smartctl_exporter. The legacy role's
// `ignore_errors: ansible_virtualization_type == 'docker'` becomes
// `Skip::InContainer`. The legacy role also gated install on
// `ansible_architecture == "x86_64"` — outside that we emit an empty marker
// and a warn. Add an arch-appropriate image / unit when arm64 hosts need
// SMART metrics.
pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    if ctx.env.architecture() != "x86_64" {
        warn!(
            architecture = %ctx.env.architecture(),
            "smartctl_exporter is x86_64-only; skipping bundle"
        );
        return ctx.plan.add(Marker {
            name: "smartctl_exporter:ready".to_string(),
            deps: vec![],
            ..Default::default()
        });
    }

    let docker_ready = ctx.docker();

    let unit = ctx.plan.add(SystemdUnit {
        name: "smartctl_exporter.service".to_string(),
        content: r"[Unit]
Description=Smartctl Exporter
After=network.target

[Service]
Type=simple
ExecStartPre=-/usr/bin/podman rm -f smartctl_exporter
ExecStart=/usr/bin/podman run --rm \
    --name smartctl_exporter \
    --privileged \
    --user root \
    --publish 9633:9633 \
    quay.io/prometheuscommunity/smartctl-exporter
ExecStop=/usr/bin/podman stop smartctl_exporter
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
        name: "smartctl_exporter.service".to_string(),
        enabled: true,
        started: true,
        restart_on: vec![unit],
        deps: vec![unit],
        skip_when: Skip::InContainer,
    });

    ctx.plan.add(Marker {
        name: "smartctl_exporter:ready".to_string(),
        deps: vec![unit, service],
        skip_when: Skip::InContainer,
    })
}
