use std::fmt::Write as _;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::apt_package::AptPackage;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::backends::service::Service;
use crate::config::{Config, IptablesPorts};
use crate::resource::{ResourceId, Skip};
use std::time::Duration;

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
        content: render_rules_v4(&RulesetInputs::from_config(ctx.config)),
        mode: Some(Permissions::from_mode(0o640)),
        deps: vec![pkg],
        ..Default::default()
    });

    let rules_v6 = ctx.plan.add(File {
        path: PathBuf::from("/etc/iptables/rules.v6"),
        content: render_rules_v6(&RulesetInputs::from_config(ctx.config)),
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

const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
const RATE_LIMIT_NEW_CONNECTIONS_PER_WINDOW: u32 = 10;
const ICMP_ECHO_RATE: &str = "5/second";
const ICMP_ECHO_BURST: u32 = 10;

const TRUSTED_INTERFACES: [&str; 1] = ["tailscale+"];
const CONTAINER_INTERFACES: [&str; 5] = ["docker+", "podman+", "lxcbr+", "virbr+", "br-+"];

#[derive(Debug)]
struct RulesetInputs<'a> {
    ports: &'a IptablesPorts,
    rate_limited_tcp_ports: &'a [u16],
}

impl<'a> RulesetInputs<'a> {
    fn from_config(config: &'a Config) -> Self {
        Self {
            ports: &config.iptables_open_ports,
            rate_limited_tcp_ports: &config.iptables_rate_limited_tcp_ports,
        }
    }
}

fn render_tcp_rate_limit(port: u16) -> String {
    let seconds = RATE_LIMIT_WINDOW.as_secs();
    format!(
        "-A NF_PERSIST_INPUT -m tcp -p tcp --dport {port} -m conntrack --ctstate NEW -m recent --name NF_PERSIST_RATE_{port} --set\n\
         -A NF_PERSIST_INPUT -m tcp -p tcp --dport {port} -m conntrack --ctstate NEW -m recent --name NF_PERSIST_RATE_{port} --update --seconds {seconds} --hitcount {RATE_LIMIT_NEW_CONNECTIONS_PER_WINDOW} -j DROP\n"
    )
}

fn render_remote_tcp(ports: &[u16], rate_limited_ports: &[u16]) -> String {
    let mut out = String::new();
    for port in ports {
        if rate_limited_ports.contains(port) {
            out.push_str(&render_tcp_rate_limit(*port));
        }
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
        for interface in TRUSTED_INTERFACES {
            let _ = writeln!(
                out,
                "-A NF_PERSIST_INPUT -i {interface} -m {protocol} -p {protocol} --dport {port} -j ACCEPT",
            );
        }
    }
    out
}

fn render_interface_accepts(chain: &str, interfaces: &[&str]) -> String {
    let mut out = String::new();
    for interface in interfaces {
        let _ = writeln!(out, "-A {chain} -i {interface} -j ACCEPT");
    }
    out
}

fn render_masquerade_returns() -> String {
    let mut out = String::new();
    for interface in CONTAINER_INTERFACES {
        let _ = writeln!(out, "-A NF_PERSIST_POSTROUTING -o {interface} -j RETURN");
    }
    out
}

fn render_nat_table() -> String {
    format!(
        "*nat
:NF_PERSIST_POSTROUTING - [0:0]
-A NF_PERSIST_POSTROUTING -m addrtype --src-type LOCAL -j RETURN
-A NF_PERSIST_POSTROUTING -o lo -j RETURN
{returns}-A NF_PERSIST_POSTROUTING -j MASQUERADE
:PREROUTING ACCEPT [0:0]
:INPUT ACCEPT [0:0]
:OUTPUT ACCEPT [0:0]
:POSTROUTING ACCEPT [0:0]
-A POSTROUTING -j NF_PERSIST_POSTROUTING
COMMIT",
        returns = render_masquerade_returns(),
    )
}

const MANGLE_TABLE: &str = r"*mangle
:PREROUTING ACCEPT [0:0]
:INPUT ACCEPT [0:0]
:FORWARD ACCEPT [0:0]
:OUTPUT ACCEPT [0:0]
:POSTROUTING ACCEPT [0:0]
COMMIT";

fn render_forward_chain() -> String {
    format!(
        ":NF_PERSIST_FORWARD - [0:0]
-A NF_PERSIST_FORWARD -m conntrack --ctstate INVALID -j DROP
-A NF_PERSIST_FORWARD -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
-A NF_PERSIST_FORWARD -i lo -j ACCEPT
{containers}-A NF_PERSIST_FORWARD -i veth+ -j ACCEPT
{trusted}-A NF_PERSIST_FORWARD -j DROP
:INPUT DROP [0:0]
-A INPUT -j NF_PERSIST_INPUT
:FORWARD DROP [0:0]
-A FORWARD -j NF_PERSIST_FORWARD
:OUTPUT ACCEPT [0:0]
COMMIT",
        containers = render_interface_accepts("NF_PERSIST_FORWARD", &CONTAINER_INTERFACES),
        trusted = render_interface_accepts("NF_PERSIST_FORWARD", &TRUSTED_INTERFACES),
    )
}

fn render_rules_v4(inputs: &RulesetInputs<'_>) -> String {
    format!(
        "{MANGLE_TABLE}
{nat}
*filter
:NF_PERSIST_INPUT - [0:0]
-A NF_PERSIST_INPUT ! -i lo -s 127.0.0.0/8 -j DROP
-A NF_PERSIST_INPUT -i lo -j ACCEPT
-A NF_PERSIST_INPUT -m conntrack --ctstate INVALID -j DROP
-A NF_PERSIST_INPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
{remote_tcp}{trusted_tcp}{remote_udp}{trusted_udp}-A NF_PERSIST_INPUT -p icmp --icmp-type 8 -m limit --limit {ICMP_ECHO_RATE} --limit-burst {ICMP_ECHO_BURST} -j ACCEPT
{containers}-A NF_PERSIST_INPUT -j DROP
{forward_chain}
",
        nat = render_nat_table(),
        remote_tcp = render_remote_tcp(&inputs.ports.remote.tcp, inputs.rate_limited_tcp_ports),
        trusted_tcp = render_trusted_ports(&inputs.ports.local.tcp, "tcp"),
        remote_udp = render_remote_udp(&inputs.ports.remote.udp),
        trusted_udp = render_trusted_ports(&inputs.ports.local.udp, "udp"),
        containers = render_interface_accepts("NF_PERSIST_INPUT", &CONTAINER_INTERFACES),
        forward_chain = render_forward_chain(),
    )
}

fn render_rules_v6(inputs: &RulesetInputs<'_>) -> String {
    format!(
        "{MANGLE_TABLE}
{nat}
*filter
:NF_PERSIST_INPUT - [0:0]
-A NF_PERSIST_INPUT ! -i lo -s ::1/128 -j DROP
-A NF_PERSIST_INPUT -i lo -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type destination-unreachable -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type packet-too-big -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type time-exceeded -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type parameter-problem -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type echo-request -m limit --limit {ICMP_ECHO_RATE} --limit-burst {ICMP_ECHO_BURST} -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type echo-reply -m limit --limit {ICMP_ECHO_RATE} --limit-burst {ICMP_ECHO_BURST} -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type router-advertisement -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type neighbor-solicitation -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type neighbor-advertisement -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type redirect -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -m conntrack --ctstate INVALID -j DROP
-A NF_PERSIST_INPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
{remote_tcp}{trusted_tcp}{remote_udp}{trusted_udp}{containers}-A NF_PERSIST_INPUT -j DROP
{forward_chain}
",
        nat = render_nat_table(),
        remote_tcp = render_remote_tcp(&inputs.ports.remote.tcp, inputs.rate_limited_tcp_ports),
        trusted_tcp = render_trusted_ports(&inputs.ports.local.tcp, "tcp"),
        remote_udp = render_remote_udp(&inputs.ports.remote.udp),
        trusted_udp = render_trusted_ports(&inputs.ports.local.udp, "udp"),
        containers = render_interface_accepts("NF_PERSIST_INPUT", &CONTAINER_INTERFACES),
        forward_chain = render_forward_chain(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IptablesPortsBySection;

    fn config_with(remote_tcp: Vec<u16>, local_tcp: Vec<u16>) -> Config {
        Config {
            iptables_open_ports: IptablesPorts {
                remote: IptablesPortsBySection {
                    tcp: remote_tcp,
                    udp: vec![],
                },
                local: IptablesPortsBySection {
                    tcp: local_tcp,
                    udp: vec![],
                },
            },
            ..Config::default()
        }
    }

    fn ansible_default_config() -> Config {
        config_with(vec![22, 2022], vec![])
    }

    fn render_both(config: &Config) -> [String; 2] {
        let inputs = RulesetInputs::from_config(config);
        [render_rules_v4(&inputs), render_rules_v6(&inputs)]
    }

    fn offset_of(ruleset: &str, needle: &str) -> usize {
        ruleset.find(needle).unwrap_or(usize::MAX)
    }

    fn find_ipv4_literal(ruleset: &str) -> Option<&str> {
        ruleset.split_whitespace().find(|token| {
            let address = token.split('/').next().unwrap_or(token);
            let mut octets = address.split('.');
            let parsed = (&mut octets)
                .take(4)
                .filter(|o| o.parse::<u8>().is_ok())
                .count();
            parsed == 4 && octets.next().is_none()
        })
    }

    #[test]
    fn builtin_input_and_forward_policies_are_drop() {
        for ruleset in render_both(&ansible_default_config()) {
            for policy in [":INPUT DROP [0:0]", ":FORWARD DROP [0:0]"] {
                assert!(ruleset.contains(policy), "missing {policy} in:\n{ruleset}");
            }
        }
    }

    #[test]
    fn forward_chain_never_accepts_toward_container_interfaces() {
        for ruleset in render_both(&config_with(vec![22], vec![8080])) {
            for interface in CONTAINER_INTERFACES {
                let rule = format!("-o {interface} -j ACCEPT");
                assert!(
                    !ruleset.contains(&rule),
                    "unsolicited inbound to containers allowed by {rule} in:\n{ruleset}"
                );
            }
            assert!(
                !ruleset.contains("-o veth+ -j ACCEPT"),
                "unsolicited inbound to containers allowed via veth in:\n{ruleset}"
            );
        }
    }

    #[test]
    fn user_chains_use_legacy_compatible_declaration_syntax() {
        for ruleset in render_both(&ansible_default_config()) {
            for chain in [
                "NF_PERSIST_INPUT",
                "NF_PERSIST_FORWARD",
                "NF_PERSIST_POSTROUTING",
            ] {
                assert!(
                    ruleset.contains(&format!(":{chain} - [0:0]")),
                    "chain {chain} not declared as `:{chain} - [0:0]` in:\n{ruleset}"
                );
            }
        }
    }

    #[test]
    fn trusted_ports_are_reachable_only_over_an_unspoofable_interface() {
        for ruleset in render_both(&config_with(vec![22], vec![8080])) {
            for line in ruleset.lines().filter(|l| l.contains("--dport 8080")) {
                assert!(
                    TRUSTED_INTERFACES
                        .iter()
                        .any(|i| line.contains(&format!("-i {i}"))),
                    "trusted-tier rule not bound to a trusted interface: {line}"
                );
            }
        }
    }

    #[test]
    fn no_rule_grants_access_by_source_address_alone() {
        for ruleset in render_both(&config_with(vec![22], vec![8080])) {
            for line in ruleset.lines().filter(|l| l.contains(" -s ")) {
                assert!(
                    line.contains("-j DROP") || line.contains("-i lo"),
                    "source address used to grant access, which is forgeable: {line}"
                );
            }
        }
    }

    #[test]
    fn ephemeral_udp_range_is_never_opened() {
        for ruleset in render_both(&config_with(vec![22], vec![8080])) {
            assert!(
                !ruleset.contains("32768:61000"),
                "ephemeral UDP range opened in:\n{ruleset}"
            );
        }
    }

    #[test]
    fn ipv6_ruleset_contains_no_ipv4_literals() {
        let [_, v6] = render_both(&config_with(vec![22], vec![8080]));
        let literal = find_ipv4_literal(&v6);
        assert!(
            literal.is_none(),
            "ip6tables-restore rejects IPv4 literals; found {literal:?} in:\n{v6}"
        );
    }

    #[test]
    fn ipv6_does_not_hardcode_an_ssh_accept() {
        let [_, v6] = render_both(&config_with(vec![], vec![]));
        assert!(
            !v6.contains("--dport 22"),
            "port 22 accepted on IPv6 despite not being configured:\n{v6}"
        );
    }

    #[test]
    fn forged_loopback_sources_are_dropped_before_any_accept() {
        let [v4, v6] = render_both(&ansible_default_config());
        for (ruleset, rule) in [
            (&v4, "-A NF_PERSIST_INPUT ! -i lo -s 127.0.0.0/8 -j DROP"),
            (&v6, "-A NF_PERSIST_INPUT ! -i lo -s ::1/128 -j DROP"),
        ] {
            let drop_at = offset_of(ruleset, rule);
            assert!(drop_at != usize::MAX, "missing {rule} in:\n{ruleset}");
            assert!(
                drop_at < offset_of(ruleset, "-j ACCEPT"),
                "{rule} appears after the first ACCEPT in:\n{ruleset}"
            );
        }
    }

    #[test]
    fn rate_limited_ports_drop_over_budget_before_accepting() {
        let v4 = render_rules_v4(&RulesetInputs::from_config(&ansible_default_config()));
        let over_budget = offset_of(&v4, "--name NF_PERSIST_RATE_22 --update");
        assert!(
            over_budget != usize::MAX,
            "no rate-limit rule for port 22 in:\n{v4}"
        );
        assert!(
            over_budget
                < offset_of(
                    &v4,
                    "-A NF_PERSIST_INPUT -m tcp -p tcp --dport 22 -j ACCEPT"
                ),
            "rate-limit drop for port 22 comes after its ACCEPT in:\n{v4}"
        );
    }

    #[test]
    fn ports_absent_from_the_rate_limit_list_are_accepted_unthrottled() {
        let v4 = render_rules_v4(&RulesetInputs::from_config(&config_with(vec![443], vec![])));
        assert!(
            !v4.contains("NF_PERSIST_RATE_443"),
            "port 443 rate-limited without being listed in:\n{v4}"
        );
    }
}
