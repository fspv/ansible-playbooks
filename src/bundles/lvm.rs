use crate::backends::apt_package::AptPackage;
use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::backends::systemd_unit::SystemdUnit;
use crate::resource::{ResourceId, Skip};

use super::Context;

pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();
    let systemd_ready = ctx.systemd();

    let lvm2 = ctx.plan.add(AptPackage {
        name: "lvm2".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let service_unit = ctx.plan.add(SystemdUnit {
        name: "lvm-raid-check.service".to_string(),
        content: r#"[Unit]
Description=Throttled rolling check of all active LVM raid LVs

[Service]
Type=oneshot
ExecStart=/bin/sh -c 'lvs --noheadings -o lv_full_name -S "lv_attr=~^[rR] && lv_active=active && raid_sync_action!=idle && raid_sync_action!=check" | xargs -r -n1 lvchange --quiet --maxrecoveryrate 0'
ExecStart=/bin/sh -c 'lvs --noheadings -o lv_full_name -S "lv_attr=~^[rR] && lv_active=active && raid_sync_action=idle" | xargs -r -n1 lvchange --quiet --maxrecoveryrate 8M'
ExecStart=/bin/sh -c 'lvs --noheadings -o lv_full_name -S "lv_attr=~^[rR] && lv_active=active && raid_sync_action=idle" | xargs -r -n1 lvchange --quiet --syncaction check'
"#
        .to_string(),
        deps: vec![lvm2, systemd_ready],
        skip_when: Skip::InContainer,
    });

    let timer_unit = ctx.plan.add(SystemdUnit {
        name: "lvm-raid-check.timer".to_string(),
        content: r"[Unit]
Description=Throttled rolling check of all active LVM raid LVs

[Timer]
OnCalendar=*:0/15
Persistent=true

[Install]
WantedBy=timers.target
"
        .to_string(),
        deps: vec![lvm2, systemd_ready],
        skip_when: Skip::InContainer,
    });

    let timer = ctx.plan.add(Service {
        name: "lvm-raid-check.timer".to_string(),
        enabled: true,
        started: true,
        restart_on: vec![service_unit, timer_unit],
        deps: vec![service_unit, timer_unit],
        skip_when: Skip::InContainer,
    });

    ctx.plan.add(Marker {
        name: "lvm:ready".to_string(),
        deps: vec![lvm2, service_unit, timer_unit, timer],
        ..Default::default()
    })
}
