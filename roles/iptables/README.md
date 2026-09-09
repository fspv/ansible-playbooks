# iptables

Renders `/etc/iptables/rules.v4` and `rules.v6` and hands them to
`netfilter-persistent`. The same rulesets are produced by the Rust bundle in
`src/bundles/iptables.rs`; the two are kept byte-identical.

## Trust model

There is no per-host interface configuration. Interfaces fall into three
classes, all named by the daemon that creates them rather than by hardware
enumeration, which is what makes one fixed list work across a fleet of unlike
machines:

| Class | Interfaces | Treatment |
| --- | --- | --- |
| Trusted | `tailscale+` | Reaches ports in `iptables_open_ports.local` |
| Container/VM | `docker+`, `podman+`, `lxcbr+`, `virbr+`, `br-+` | Reaches all host ports |
| Everything else | physical NICs, anything unrecognised | Reaches only `iptables_open_ports.remote` |

Physical interfaces are never trusted, so it does not matter whether a host
names its NIC `eth0`, `enp3s0` or `wlp2s0`. That is deliberate: which physical
interface faces the internet cannot be derived reliably — a cloud VM's default
route carries an RFC1918 address behind 1:1 NAT, and a multihomed host has no
single WAN — so the rules never try to guess.

`tailscale+` is the trusted tier because WireGuard authenticates every packet.
A source-address match is not a substitute: nothing here sets `rp_filter`, and
strict `rp_filter` would not help a single-homed host anyway, since the default
route makes a forged RFC1918 source pass the reverse-path check. No rule in
either family grants access on source address alone.

## Ports

- `iptables_open_ports.remote` — open to the world.
- `iptables_open_ports.local` — reachable only over `tailscale+`.

## Invariants

- Built-in `INPUT`/`FORWARD` policies are `DROP`, so a ruleset that fails to
  load leaves the host closed. While the `NF_PERSIST_*` chains are intact this
  is behaviourally a no-op — their terminal `DROP` already covers it — so it
  costs nothing and pays out on the failure paths.
- User chains are declared `:NAME - [0:0]`. The nft backend also accepts
  `:NAME [0:0]`, but xtables-legacy reads that as a policy on a non-built-in
  chain and rejects the whole file, which with an `ACCEPT` policy would remove
  the firewall silently.
- In `rules.v6` the ICMPv6 accepts precede the state rules, and the state rules
  precede the port accepts. conntrack does not track NDP, so NDP would be
  classed `INVALID`; and an out-of-state packet must not reach an open port.
- `rules.v6` contains no IPv4 literals. Emitting one makes `ip6tables-restore`
  reject the file and leaves IPv6 unfiltered.

The templates carry no jinja beyond the four loops over `iptables_open_ports`.
Everything else is written out literally, so the file reads as the ruleset it
produces.
