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

// Mirrors roles/iptables/. Disabled in containers — netfilter-persistent's
// systemd unit is not usable inside a container and the legacy role gates
// the service start with `ignore_errors: ansible_virtualization_type ==
// 'docker'`. We translate that to "skip the whole bundle in containers".
//
// `rules.v4` / `rules.v6` render the same rulesets as the jinja templates
// under `roles/iptables/templates/`; the two must be kept in step.
//
// Three properties carry the security of these rulesets and are easy to
// undo by accident:
//
//   - Built-in INPUT/FORWARD policies are DROP, so a ruleset that fails to
//     load leaves the host closed rather than open. The terminal DROP in
//     each NF_PERSIST_* chain makes this behaviourally a no-op while the
//     chains are intact — it only pays out when they are not.
//   - Every rule that extends LAN-level trust is bound to an interface from
//     `iptables_lan_interfaces` (empty by default). A source-address match
//     alone is not a boundary: nothing here sets rp_filter, and strict
//     rp_filter would not help a single-homed host anyway, since the
//     default route makes a forged RFC1918 source pass the reverse-path
//     check.
//   - The FORWARD chain accepts new connections only *from* container/VM
//     interfaces, never toward them. Docker runs with `"iptables": false`
//     (see bundles/docker.rs), so no DOCKER/DOCKER-USER chain exists and
//     these rules are the whole container network boundary.
//
// User chains are declared `:NAME - [0:0]`, not `:NAME [0:0]`: the nft
// backend accepts both, but xtables-legacy reads the latter as a policy on
// a non-built-in chain and rejects the entire file.
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

/// Window over which `iptables_rate_limited_tcp_ports` counts new
/// connections from a single source address.
const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60);
/// New connections allowed per source address per [`RATE_LIMIT_WINDOW`].
const RATE_LIMIT_NEW_CONNECTIONS_PER_WINDOW: u32 = 10;
/// Sustained rate and burst allowance for inbound echo requests.
const ICMP_ECHO_RATE: &str = "5/second";
const ICMP_ECHO_BURST: u32 = 10;

/// RFC1918 ranges treated as LAN sources, checked in addition to the
/// interface binding. IPv6 has no equivalent worth matching on: a LAN with
/// ISP-delegated addressing uses globals, not ULA, so a prefix match there
/// would reject legitimate traffic while adding nothing the interface
/// binding does not already give.
const LAN_SOURCE_RANGES_V4: [&str; 3] = ["192.168.0.0/16", "172.16.0.0/12", "10.0.0.0/8"];

/// Everything the two renderers read, borrowed from `Config`.
#[derive(Debug)]
struct RulesetInputs<'a> {
    ports: &'a IptablesPorts,
    lan_interfaces: &'a [String],
    allow_chromecast: bool,
    rate_limited_tcp_ports: &'a [u16],
}

impl<'a> RulesetInputs<'a> {
    fn from_config(config: &'a Config) -> Self {
        Self {
            ports: &config.iptables_open_ports,
            lan_interfaces: &config.iptables_lan_interfaces,
            allow_chromecast: config.iptables_allow_chromecast,
            rate_limited_tcp_ports: &config.iptables_rate_limited_tcp_ports,
        }
    }
}

/// Two rules that count new connections per source address and drop the
/// ones past the budget, emitted ahead of the port's ACCEPT so the ACCEPT is
/// only reached by connections within budget.
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

fn render_local_tcp_v4(ports: &[u16], lan_interfaces: &[String]) -> String {
    let mut out = String::new();
    for port in ports {
        for interface in lan_interfaces {
            for range in LAN_SOURCE_RANGES_V4 {
                let _ = writeln!(
                    out,
                    "-A NF_PERSIST_INPUT -i {interface} -m tcp -p tcp -s {range} --dport {port} -j ACCEPT",
                );
            }
        }
    }
    out
}

fn render_local_udp_v4(ports: &[u16], lan_interfaces: &[String]) -> String {
    let mut out = String::new();
    for port in ports {
        for interface in lan_interfaces {
            for range in LAN_SOURCE_RANGES_V4 {
                let _ = writeln!(
                    out,
                    "-A NF_PERSIST_INPUT -i {interface} -m udp -p udp -s {range} --dport {port} -j ACCEPT",
                );
            }
        }
    }
    out
}

fn render_local_tcp_v6(ports: &[u16], lan_interfaces: &[String]) -> String {
    let mut out = String::new();
    for port in ports {
        for interface in lan_interfaces {
            let _ = writeln!(
                out,
                "-A NF_PERSIST_INPUT -i {interface} -m tcp -p tcp --dport {port} -j ACCEPT",
            );
        }
    }
    out
}

fn render_local_udp_v6(ports: &[u16], lan_interfaces: &[String]) -> String {
    let mut out = String::new();
    for port in ports {
        for interface in lan_interfaces {
            let _ = writeln!(
                out,
                "-A NF_PERSIST_INPUT -i {interface} -m udp -p udp --dport {port} -j ACCEPT",
            );
        }
    }
    out
}

fn render_chromecast(allow: bool, lan_interfaces: &[String]) -> String {
    if !allow {
        return String::new();
    }
    let mut out = String::new();
    for interface in lan_interfaces {
        for range in ["192.168.0.0/16", "10.0.0.0/8", "172.16.0.0/12"] {
            let _ = writeln!(
                out,
                "-A NF_PERSIST_INPUT -i {interface} -s {range} -p udp -m multiport --sports 32768:61000 -m multiport --dports 32768:61000 -m comment --comment \"Allow Chromecast UDP data (inbound)\" -j ACCEPT",
            );
        }
    }
    out
}

fn render_lan_forward_accepts(lan_interfaces: &[String]) -> String {
    let mut out = String::new();
    for interface in lan_interfaces {
        let _ = writeln!(out, "-A NF_PERSIST_FORWARD -i {interface} -j ACCEPT");
    }
    out
}

/// Identical in both families, so it lives in one place rather than being
/// spelled twice in the two format strings.
const NAT_TABLE: &str = r"*nat
:NF_PERSIST_POSTROUTING - [0:0]
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
COMMIT";

const MANGLE_TABLE: &str = r"*mangle
:PREROUTING ACCEPT [0:0]
:INPUT ACCEPT [0:0]
:FORWARD ACCEPT [0:0]
:OUTPUT ACCEPT [0:0]
:POSTROUTING ACCEPT [0:0]
COMMIT";

fn render_forward_chain(lan_interfaces: &[String]) -> String {
    format!(
        ":NF_PERSIST_FORWARD - [0:0]
-A NF_PERSIST_FORWARD -m conntrack --ctstate INVALID -j DROP
# Return traffic for connections a container or VM opened outbound.
-A NF_PERSIST_FORWARD -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
# New connections may originate FROM a container/VM interface, never toward
# one: docker runs with \"iptables\": false, so these rules are the only thing
# standing between an off-host packet and a container's ports.
-A NF_PERSIST_FORWARD -i lo -j ACCEPT
-A NF_PERSIST_FORWARD -i docker+ -j ACCEPT
-A NF_PERSIST_FORWARD -i lxcbr+ -j ACCEPT
-A NF_PERSIST_FORWARD -i virbr+ -j ACCEPT
-A NF_PERSIST_FORWARD -i br-+ -j ACCEPT
-A NF_PERSIST_FORWARD -i veth+ -j ACCEPT
{lan_accepts}-A NF_PERSIST_FORWARD -j DROP
:INPUT DROP [0:0]
-A INPUT -j NF_PERSIST_INPUT
:FORWARD DROP [0:0]
-A FORWARD -j NF_PERSIST_FORWARD
:OUTPUT ACCEPT [0:0]
COMMIT",
        lan_accepts = render_lan_forward_accepts(lan_interfaces),
    )
}

fn render_rules_v4(inputs: &RulesetInputs<'_>) -> String {
    format!(
        "{MANGLE_TABLE}
{NAT_TABLE}
*filter
:NF_PERSIST_INPUT - [0:0]
# A loopback source address is only ever legitimate on lo; arriving anywhere
# else it is forged. Must precede every ACCEPT below.
-A NF_PERSIST_INPUT ! -i lo -s 127.0.0.0/8 -j DROP
-A NF_PERSIST_INPUT -i lo -j ACCEPT
-A NF_PERSIST_INPUT -m conntrack --ctstate INVALID -j DROP
-A NF_PERSIST_INPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
{remote_tcp}{local_tcp}{remote_udp}{local_udp}-A NF_PERSIST_INPUT -p icmp --icmp-type 8 -m limit --limit {ICMP_ECHO_RATE} --limit-burst {ICMP_ECHO_BURST} -j ACCEPT
-A NF_PERSIST_INPUT -i docker+ -j ACCEPT
-A NF_PERSIST_INPUT -i lxcbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i virbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i br-+ -j ACCEPT
{chromecast}-A NF_PERSIST_INPUT -j DROP
{forward_chain}
",
        remote_tcp = render_remote_tcp(&inputs.ports.remote.tcp, inputs.rate_limited_tcp_ports),
        local_tcp = render_local_tcp_v4(&inputs.ports.local.tcp, inputs.lan_interfaces),
        remote_udp = render_remote_udp(&inputs.ports.remote.udp),
        local_udp = render_local_udp_v4(&inputs.ports.local.udp, inputs.lan_interfaces),
        chromecast = render_chromecast(inputs.allow_chromecast, inputs.lan_interfaces),
        forward_chain = render_forward_chain(inputs.lan_interfaces),
    )
}

fn render_rules_v6(inputs: &RulesetInputs<'_>) -> String {
    format!(
        "{MANGLE_TABLE}
{NAT_TABLE}
*filter
:NF_PERSIST_INPUT - [0:0]
# A loopback source address is only ever legitimate on lo; arriving anywhere
# else it is forged. Must precede every ACCEPT below.
-A NF_PERSIST_INPUT ! -i lo -s ::1/128 -j DROP
-A NF_PERSIST_INPUT -i lo -j ACCEPT

# ICMPv6 is accepted ahead of the INVALID drop below: conntrack does not
# track NDP, so neighbour solicit/advert would be classed INVALID and
# dropping them first would break IPv6 address resolution outright.

-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type destination-unreachable -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type packet-too-big -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type time-exceeded -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type parameter-problem -j ACCEPT

-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type echo-request -m limit --limit {ICMP_ECHO_RATE} --limit-burst {ICMP_ECHO_BURST} -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type echo-reply -m limit --limit {ICMP_ECHO_RATE} --limit-burst {ICMP_ECHO_BURST} -j ACCEPT

# Allow others ICMPv6 types but only if the hop limit field is 255.

-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type router-advertisement -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type neighbor-solicitation -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type neighbor-advertisement -m hl --hl-eq 255 -j ACCEPT
-A NF_PERSIST_INPUT -p icmpv6 --icmpv6-type redirect -m hl --hl-eq 255 -j ACCEPT

-A NF_PERSIST_INPUT -m conntrack --ctstate INVALID -j DROP
-A NF_PERSIST_INPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
{remote_tcp}{local_tcp}{remote_udp}{local_udp}-A NF_PERSIST_INPUT -i docker+ -j ACCEPT
-A NF_PERSIST_INPUT -i lxcbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i virbr+ -j ACCEPT
-A NF_PERSIST_INPUT -i br-+ -j ACCEPT
-A NF_PERSIST_INPUT -j DROP
{forward_chain}
",
        remote_tcp = render_remote_tcp(&inputs.ports.remote.tcp, inputs.rate_limited_tcp_ports),
        local_tcp = render_local_tcp_v6(&inputs.ports.local.tcp, inputs.lan_interfaces),
        remote_udp = render_remote_udp(&inputs.ports.remote.udp),
        local_udp = render_local_udp_v6(&inputs.ports.local.udp, inputs.lan_interfaces),
        forward_chain = render_forward_chain(inputs.lan_interfaces),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IptablesPortsBySection;

    fn config_with(
        remote_tcp: Vec<u16>,
        local_tcp: Vec<u16>,
        lan_interfaces: Vec<String>,
    ) -> Config {
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
            iptables_lan_interfaces: lan_interfaces,
            ..Config::default()
        }
    }

    fn ansible_default_config() -> Config {
        config_with(vec![22, 2022], vec![], vec![])
    }

    fn lan_config() -> Config {
        config_with(vec![22], vec![8080], vec!["enp3s0".to_string()])
    }

    fn render_both(config: &Config) -> [String; 2] {
        let inputs = RulesetInputs::from_config(config);
        [render_rules_v4(&inputs), render_rules_v6(&inputs)]
    }

    /// Byte offset of `needle`, or `usize::MAX` when absent, so ordering
    /// assertions can report a missing rule instead of panicking on unwrap.
    fn offset_of(ruleset: &str, needle: &str) -> usize {
        ruleset.find(needle).unwrap_or(usize::MAX)
    }

    /// A whitespace-separated token that parses as a dotted quad, ignoring
    /// any `/prefix` suffix.
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
        for ruleset in render_both(&lan_config()) {
            for bad in [
                "-o docker+ -j ACCEPT",
                "-o veth+ -j ACCEPT",
                "-o br-+ -j ACCEPT",
            ] {
                assert!(
                    !ruleset.contains(bad),
                    "unsolicited inbound to containers allowed by {bad} in:\n{ruleset}"
                );
            }
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
    fn no_lan_trust_is_granted_without_configured_lan_interfaces() {
        let config = config_with(vec![22], vec![8080], vec![]);
        for ruleset in render_both(&config) {
            assert!(
                !ruleset.contains("8080"),
                "local port opened with no trusted interface in:\n{ruleset}"
            );
            for range in LAN_SOURCE_RANGES_V4 {
                assert!(
                    !ruleset.contains(range),
                    "source range {range} trusted with no trusted interface in:\n{ruleset}"
                );
            }
        }
    }

    #[test]
    fn local_ports_are_bound_to_a_lan_interface() {
        for ruleset in render_both(&lan_config()) {
            for line in ruleset.lines().filter(|l| l.contains("--dport 8080")) {
                assert!(
                    line.contains("-i enp3s0"),
                    "local port rule not bound to an interface: {line}"
                );
            }
        }
    }

    #[test]
    fn chromecast_range_requires_explicit_opt_in() {
        let mut config = lan_config();
        assert!(
            !render_rules_v4(&RulesetInputs::from_config(&config)).contains("32768:61000"),
            "ephemeral UDP range opened without iptables_allow_chromecast"
        );
        config.iptables_allow_chromecast = true;
        let opted_in = render_rules_v4(&RulesetInputs::from_config(&config));
        for line in opted_in.lines().filter(|l| l.contains("32768:61000")) {
            assert!(
                line.contains("-i enp3s0"),
                "chromecast rule not bound to an interface: {line}"
            );
        }
    }

    #[test]
    fn ipv6_ruleset_contains_no_ipv4_literals() {
        let config = config_with(vec![22], vec![8080], vec!["enp3s0".to_string()]);
        let [_, v6] = render_both(&config);
        let literal = find_ipv4_literal(&v6);
        assert!(
            literal.is_none(),
            "ip6tables-restore rejects IPv4 literals; found {literal:?} in:\n{v6}"
        );
    }

    #[test]
    fn ipv6_does_not_hardcode_an_ssh_accept() {
        let [_, v6] = render_both(&config_with(vec![], vec![], vec![]));
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
            let first_accept = offset_of(ruleset, "-j ACCEPT");
            assert!(
                drop_at < first_accept,
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
        let accept = offset_of(
            &v4,
            "-A NF_PERSIST_INPUT -m tcp -p tcp --dport 22 -j ACCEPT",
        );
        assert!(
            over_budget < accept,
            "rate-limit drop for port 22 comes after its ACCEPT in:\n{v4}"
        );
    }

    #[test]
    fn ports_absent_from_the_rate_limit_list_are_accepted_unthrottled() {
        let config = config_with(vec![443], vec![], vec![]);
        let v4 = render_rules_v4(&RulesetInputs::from_config(&config));
        assert!(
            !v4.contains("NF_PERSIST_RATE_443"),
            "port 443 rate-limited without being listed in:\n{v4}"
        );
    }
}
