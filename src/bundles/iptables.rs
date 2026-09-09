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

fn render_trusted_ports(ports: &[u16], protocol: &str) -> String {
    let mut out = String::new();
    for port in ports {
        let _ = writeln!(
            out,
            "-A NF_PERSIST_INPUT -i tailscale+ -m {protocol} -p {protocol} --dport {port} -j ACCEPT",
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
:NF_PERSIST_POSTROUTING - [0:0]
-A NF_PERSIST_POSTROUTING -m addrtype --src-type LOCAL -j RETURN
-A NF_PERSIST_POSTROUTING -o lo -j RETURN
-A NF_PERSIST_POSTROUTING -o docker+ -j RETURN
-A NF_PERSIST_POSTROUTING -o podman+ -j RETURN
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
:NF_PERSIST_INPUT - [0:0]
-A NF_PERSIST_INPUT ! -i lo -s 127.0.0.0/8 -j DROP
-A NF_PERSIST_INPUT -i lo -j ACCEPT
-A NF_PERSIST_INPUT -m conntrack --ctstate INVALID -j DROP
-A NF_PERSIST_INPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
{remote_tcp}{trusted_tcp}{remote_udp}{trusted_udp}-A NF_PERSIST_INPUT -p icmp --icmp-type 8 -j ACCEPT
-A NF_PERSIST_INPUT -i docker+ -j ACCEPT
-A NF_PERSIST_INPUT -i podman+ -j ACCEPT
-A NF_PERSIST_INPUT -i lxcbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i virbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i br-+ -j ACCEPT
-A NF_PERSIST_INPUT -j DROP
:NF_PERSIST_FORWARD - [0:0]
-A NF_PERSIST_FORWARD -m conntrack --ctstate INVALID -j DROP
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
:INPUT DROP [0:0]
-A INPUT -j NF_PERSIST_INPUT
:FORWARD DROP [0:0]
-A FORWARD -j NF_PERSIST_FORWARD
:OUTPUT ACCEPT [0:0]
COMMIT

",
        remote_tcp = render_remote_tcp(&ports.remote.tcp),
        trusted_tcp = render_trusted_ports(&ports.local.tcp, "tcp"),
        remote_udp = render_remote_udp(&ports.remote.udp),
        trusted_udp = render_trusted_ports(&ports.local.udp, "udp"),
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
:NF_PERSIST_POSTROUTING - [0:0]
-A NF_PERSIST_POSTROUTING -m addrtype --src-type LOCAL -j RETURN
-A NF_PERSIST_POSTROUTING -o lo -j RETURN
-A NF_PERSIST_POSTROUTING -o docker+ -j RETURN
-A NF_PERSIST_POSTROUTING -o podman+ -j RETURN
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
:NF_PERSIST_INPUT - [0:0]
-A NF_PERSIST_INPUT ! -i lo -s ::1/128 -j DROP
-A NF_PERSIST_INPUT -i lo -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type destination-unreachable -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type packet-too-big -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type time-exceeded -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type parameter-problem -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type echo-request -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type echo-reply -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type router-advertisement -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type neighbor-solicitation -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type neighbor-advertisement -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type redirect -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -m conntrack --ctstate INVALID -j DROP
-A NF_PERSIST_INPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
{remote_tcp}{trusted_tcp}{remote_udp}{trusted_udp}
-A NF_PERSIST_INPUT -i docker+ -j ACCEPT
-A NF_PERSIST_INPUT -i podman+ -j ACCEPT
-A NF_PERSIST_INPUT -i lxcbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i virbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i br-+ -j ACCEPT
-A NF_PERSIST_INPUT -j DROP
:NF_PERSIST_FORWARD - [0:0]
-A NF_PERSIST_FORWARD -m conntrack --ctstate INVALID -j DROP
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
:INPUT DROP [0:0]
-A INPUT -j NF_PERSIST_INPUT
:FORWARD DROP [0:0]
-A FORWARD -j NF_PERSIST_FORWARD
:OUTPUT ACCEPT [0:0]
COMMIT
",
        remote_tcp = render_remote_tcp(&ports.remote.tcp),
        trusted_tcp = render_trusted_ports(&ports.local.tcp, "tcp"),
        remote_udp = render_remote_udp(&ports.remote.udp),
        trusted_udp = render_trusted_ports(&ports.local.udp, "udp"),
    )
}
