use std::fmt::Write as _;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::apt_package::AptPackage;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::config::IptablesPorts;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/iptables/. Disabled in containers — netfilter-persistent's
// systemd unit is not usable inside a container and the legacy role gates
// the service start with `ignore_errors: ansible_virtualization_type ==
// 'docker'`. We translate that to "skip the whole bundle in containers".
//
// `rules.v4` / `rules.v6` are templated from `ctx.config.iptables_open_ports`
// matching the legacy Jinja exactly: a hardcoded `--dport 22` accept line in
// v6 only, plus loops over `remote.tcp`, `local.tcp`, `remote.udp`,
// `local.udp` that emit ACCEPT lines per port. With an empty
// `iptables_open_ports`, the loops contribute nothing and only the static
// frame remains; with the legacy default `{ remote: { tcp: [22, 2022] } }`
// the v6 file ends up with port 22 listed twice (hardcoded + first loop
// iteration) followed by 2022, byte-identical to what the ansible role
// would produce.
//
// Bundle dep: legacy `roles/iptables/meta/main.yml` requires the tailscale
// role first so the tailscale package (and any apt repo it brings) is
// installed before netfilter-persistent comes up.

#[allow(clippy::too_many_lines)]
pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    if ctx.env.is_container() {
        return ctx.plan.add(Marker {
            name: "iptables:ready".to_string(),
            deps: vec![],
            ..Default::default()
        });
    }

    let apt_ready = ctx.apt();
    let tailscale_ready = ctx.tailscale();

    let pkg = ctx.plan.add(AptPackage {
        name: "iptables-persistent".to_string(),
        deps: vec![apt_ready, tailscale_ready],
        ..Default::default()
    });

    let netfilter_default = ctx.plan.add(File {
        path: PathBuf::from("/etc/default/netfilter-persistent"),
        content: "FLUSH_ON_STOP=0\n\
                  IPTABLES_SKIP_SAVE=yes\n\
                  IP6TABLES_SKIP_SAVE=yes\n\
                  IPSET_SKIP_SAVE=yes\n\
                  IPTABLES_RESTORE_NOFLUSH=yes\n\
                  IP6TABLES_RESTORE_NOFLUSH=yes\n\
                  IPTABLES_TEST_RULESET=yes\n\
                  IP6TABLES_TEST_RULESET=yes\n"
            .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![pkg],
        ..Default::default()
    });

    let rules_v4 = ctx.plan.add(File {
        path: PathBuf::from("/etc/iptables/rules.v4"),
        content: render_rules_v4(&ctx.config.iptables_open_ports),
        mode: Some(Permissions::from_mode(0o640)),
        deps: vec![pkg],
        ..Default::default()
    });

    let rules_v6 = ctx.plan.add(File {
        path: PathBuf::from("/etc/iptables/rules.v6"),
        content: render_rules_v6(&ctx.config.iptables_open_ports),
        mode: Some(Permissions::from_mode(0o640)),
        deps: vec![pkg],
        ..Default::default()
    });

    let service = ctx.plan.add(Service {
        name: "netfilter-persistent.service".to_string(),
        enabled: true,
        started: true,
        restart_on: vec![netfilter_default, rules_v4, rules_v6],
        deps: vec![pkg, netfilter_default, rules_v4, rules_v6],
        skip_when: Skip::InContainer,
    });

    ctx.plan.add(Marker {
        name: "iptables:ready".to_string(),
        deps: vec![pkg, netfilter_default, rules_v4, rules_v6, service],
        ..Default::default()
    })
}

fn render_remote_tcp(ports: &[u16]) -> String {
    let mut out = String::new();
    for port in ports {
        let _ = writeln!(
            out,
            "-A NF_PERSIST_INPUT -m tcp -p tcp --dport {port} -j ACCEPT",
        );
    }
    out
}

fn render_local_tcp(ports: &[u16]) -> String {
    let mut out = String::new();
    for port in ports {
        let _ = writeln!(
            out,
            "-A NF_PERSIST_INPUT -m tcp -p tcp -s 192.168.0.0/8 --dport {port} -j ACCEPT",
        );
        let _ = writeln!(
            out,
            "-A NF_PERSIST_INPUT -m tcp -p tcp -s 172.16.0.0/12 --dport {port} -j ACCEPT",
        );
        let _ = writeln!(
            out,
            "-A NF_PERSIST_INPUT -m tcp -p tcp -s 10.0.0.0/8 --dport {port} -j ACCEPT",
        );
    }
    out
}

fn render_remote_udp(ports: &[u16]) -> String {
    let mut out = String::new();
    for port in ports {
        let _ = writeln!(
            out,
            "-A NF_PERSIST_INPUT -m udp -p udp --dport {port} -j ACCEPT",
        );
    }
    out
}

fn render_local_udp(ports: &[u16]) -> String {
    let mut out = String::new();
    for port in ports {
        let _ = writeln!(
            out,
            "-A NF_PERSIST_INPUT -m udp -p udp -s 192.168.0.0/8 --dport {port} -j ACCEPT",
        );
        let _ = writeln!(
            out,
            "-A NF_PERSIST_INPUT -m udp -p udp -s 172.16.0.0/12 --dport {port} -j ACCEPT",
        );
        let _ = writeln!(
            out,
            "-A NF_PERSIST_INPUT -m udp -p udp -s 10.0.0.0/8 --dport {port} -j ACCEPT",
        );
    }
    out
}

fn render_rules_v4(ports: &IptablesPorts) -> String {
    format!(
        "*mangle
:PREROUTING ACCEPT [0:0]
:INPUT ACCEPT [0:0]
:FORWARD ACCEPT [0:0]
:OUTPUT ACCEPT [0:0]
:POSTROUTING ACCEPT [0:0]
COMMIT
*nat
:NF_PERSIST_POSTROUTING [0:0]
# Do not forward locally generated packets
-A NF_PERSIST_POSTROUTING -m addrtype --src-type LOCAL -j RETURN

# Do not forward packets to internal networks (for security reasons)
-A NF_PERSIST_POSTROUTING -o lo -j RETURN
-A NF_PERSIST_POSTROUTING -o docker+ -j RETURN
-A NF_PERSIST_POSTROUTING -o lxcbr+ -j RETURN
-A NF_PERSIST_POSTROUTING -o virbr+ -j RETURN
-A NF_PERSIST_POSTROUTING -o br-+ -j RETURN

-A NF_PERSIST_POSTROUTING -j MASQUERADE
:PREROUTING ACCEPT [0:0]
:INPUT ACCEPT [0:0]
:OUTPUT ACCEPT [0:0]
:POSTROUTING ACCEPT [0:0]
-A POSTROUTING -j NF_PERSIST_POSTROUTING
COMMIT
*filter
:NF_PERSIST_INPUT [0:0]
-A NF_PERSIST_INPUT -m addrtype --src-type LOCAL -d 127.0.0.0/24 -j ACCEPT
-A NF_PERSIST_INPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
{remote_tcp}{local_tcp}{remote_udp}{local_udp}-A NF_PERSIST_INPUT -p icmp --icmp-type 8 -j ACCEPT
-A NF_PERSIST_INPUT -i lo -j ACCEPT
-A NF_PERSIST_INPUT -i docker+ -j ACCEPT
-A NF_PERSIST_INPUT -i lxcbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i virbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i br-+ -j ACCEPT
-A NF_PERSIST_INPUT -s 192.168.0.0/8 -p udp -m multiport --sports 32768:61000 -m multiport --dports 32768:61000 -m comment --comment \"Allow Chromecast UDP data (inbound)\" -j ACCEPT
-A NF_PERSIST_INPUT -s 10.0.0.0/8 -p udp -m multiport --sports 32768:61000 -m multiport --dports 32768:61000 -m comment --comment \"Allow Chromecast UDP data (inbound)\" -j ACCEPT
-A NF_PERSIST_INPUT -s 172.16.0.0/12 -p udp -m multiport --sports 32768:61000 -m multiport --dports 32768:61000 -m comment --comment \"Allow Chromecast UDP data (inbound)\" -j ACCEPT
-A NF_PERSIST_INPUT -j DROP
:NF_PERSIST_FORWARD [0:0]
# Do not forward packets from interfaces not identified as local
-A NF_PERSIST_FORWARD -i lo -j ACCEPT
-A NF_PERSIST_FORWARD -o lo -j ACCEPT
-A NF_PERSIST_FORWARD -i docker+ -j ACCEPT
-A NF_PERSIST_FORWARD -o docker+ -j ACCEPT
-A NF_PERSIST_FORWARD -i lxcbr+ -j ACCEPT
-A NF_PERSIST_FORWARD -o lxcbr+ -j ACCEPT
-A NF_PERSIST_FORWARD -i virbr+ -j ACCEPT
-A NF_PERSIST_FORWARD -o virbr+ -j ACCEPT
-A NF_PERSIST_FORWARD -i br-+ -j ACCEPT
-A NF_PERSIST_FORWARD -o br-+ -j ACCEPT
-A NF_PERSIST_FORWARD -i veth+ -j ACCEPT
-A NF_PERSIST_FORWARD -o veth+ -j ACCEPT
-A NF_PERSIST_FORWARD -j DROP
:INPUT ACCEPT [0:0]
-A INPUT -j NF_PERSIST_INPUT
:FORWARD ACCEPT [0:0]
-A FORWARD -j NF_PERSIST_FORWARD
:OUTPUT ACCEPT [0:0]
COMMIT

",
        remote_tcp = render_remote_tcp(&ports.remote.tcp),
        local_tcp = render_local_tcp(&ports.local.tcp),
        remote_udp = render_remote_udp(&ports.remote.udp),
        local_udp = render_local_udp(&ports.local.udp),
    )
}

fn render_rules_v6(ports: &IptablesPorts) -> String {
    format!(
        "*mangle
:PREROUTING ACCEPT [0:0]
:INPUT ACCEPT [0:0]
:FORWARD ACCEPT [0:0]
:OUTPUT ACCEPT [0:0]
:POSTROUTING ACCEPT [0:0]
COMMIT
*nat
:NF_PERSIST_POSTROUTING [0:0]
# Do not forward locally generated packets
-A NF_PERSIST_POSTROUTING -m addrtype --src-type LOCAL -j RETURN

# Do not forward packets to internal networks (for security reasons)
-A NF_PERSIST_POSTROUTING -o lo -j RETURN
-A NF_PERSIST_POSTROUTING -o docker+ -j RETURN
-A NF_PERSIST_POSTROUTING -o lxcbr+ -j RETURN
-A NF_PERSIST_POSTROUTING -o virbr+ -j RETURN
-A NF_PERSIST_POSTROUTING -o br-+ -j RETURN

-A NF_PERSIST_POSTROUTING -j MASQUERADE
:PREROUTING ACCEPT [0:0]
:INPUT ACCEPT [0:0]
:OUTPUT ACCEPT [0:0]
:POSTROUTING ACCEPT [0:0]
-A POSTROUTING -j NF_PERSIST_POSTROUTING
COMMIT
*filter
:NF_PERSIST_INPUT [0:0]
-A NF_PERSIST_INPUT -m addrtype --src-type LOCAL -d ::1/128 -j ACCEPT
-A NF_PERSIST_INPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
-A NF_PERSIST_INPUT -m tcp -p tcp --dport 22 -j ACCEPT
{remote_tcp}{local_tcp}{remote_udp}{local_udp}
# Allow some ICMPv6 types in the INPUT chain
# Using ICMPv6 type names to be clear.

-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type destination-unreachable -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type packet-too-big -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type time-exceeded -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type parameter-problem -j ACCEPT

# Allow some other types in the INPUT chain
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type echo-request -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type echo-reply -j ACCEPT

# Allow others ICMPv6 types but only if the hop limit field is 255.

-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type router-advertisement -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type neighbor-solicitation -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type neighbor-advertisement -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type redirect -m hl --hl-eq 255 -j ACCEPT

-A NF_PERSIST_INPUT -i lo -j ACCEPT
-A NF_PERSIST_INPUT -i docker+ -j ACCEPT
-A NF_PERSIST_INPUT -i lxcbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i virbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i br-+ -j ACCEPT
-A NF_PERSIST_INPUT -j DROP
:NF_PERSIST_FORWARD [0:0]
# Do not forward packets from interfaces not identified as local
-A NF_PERSIST_FORWARD -i lo -j ACCEPT
-A NF_PERSIST_FORWARD -o lo -j ACCEPT
-A NF_PERSIST_FORWARD -i docker+ -j ACCEPT
-A NF_PERSIST_FORWARD -o docker+ -j ACCEPT
-A NF_PERSIST_FORWARD -i lxcbr+ -j ACCEPT
-A NF_PERSIST_FORWARD -o lxcbr+ -j ACCEPT
-A NF_PERSIST_FORWARD -i virbr+ -j ACCEPT
-A NF_PERSIST_FORWARD -o virbr+ -j ACCEPT
-A NF_PERSIST_FORWARD -i br-+ -j ACCEPT
-A NF_PERSIST_FORWARD -o br-+ -j ACCEPT
-A NF_PERSIST_FORWARD -j DROP
:INPUT ACCEPT [0:0]
-A INPUT -j NF_PERSIST_INPUT
:FORWARD ACCEPT [0:0]
-A FORWARD -j NF_PERSIST_FORWARD
:OUTPUT ACCEPT [0:0]
COMMIT

",
        remote_tcp = render_remote_tcp(&ports.remote.tcp),
        local_tcp = render_local_tcp(&ports.local.tcp),
        remote_udp = render_remote_udp(&ports.remote.udp),
        local_udp = render_local_udp(&ports.local.udp),
    )
}
