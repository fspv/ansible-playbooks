# iptables

Renders `/etc/iptables/rules.v4` and `rules.v6` and hands them to
`netfilter-persistent`. The same rulesets are produced by the Rust bundle in
`src/bundles/iptables.rs`; the two are kept byte-identical and unit tests in
that file pin the properties below.

## Trust model

There is no per-host interface configuration. Interfaces fall into three
hardcoded classes, all named by the daemon that creates them rather than by
hardware enumeration, which is what makes a single hardcoded list work across
a fleet of unlike machines:

| Class | Patterns | Treatment |
| --- | --- | --- |
| Trusted | `tailscale+` | Reaches ports in `iptables_open_ports.local` |
| Container/VM | `docker+`, `podman+`, `lxcbr+`, `virbr+`, `br-+` | Reaches all host ports; may originate forwarded connections |
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
- `iptables_open_ports.local` — reachable only over the trusted interfaces.
- `iptables_rate_limited_tcp_ports` — new connections from one source address
  are capped at 10 per 60s. Applied only to ports actually opened. Set to `[]`
  to disable.

## Invariants

- Built-in `INPUT`/`FORWARD` policies are `DROP`, so a ruleset that fails to
  load leaves the host closed. While the `NF_PERSIST_*` chains are intact this
  is behaviourally a no-op — their terminal `DROP` already covers it — so it
  costs nothing and pays out on the failure paths.
- User chains are declared `:NAME - [0:0]`. The nft backend also accepts
  `:NAME [0:0]`, but xtables-legacy reads that as a policy on a non-built-in
  chain and rejects the whole file, which with an `ACCEPT` policy would remove
  the firewall silently.
- `FORWARD` accepts new connections only *from* container/VM interfaces, never
  toward them; return traffic comes from the conntrack rule. Docker runs with
  `"iptables": false`, so no `DOCKER-USER` chain exists and these rules are the
  entire container network boundary.
- `rules.v6` contains no IPv4 literals. Emitting one makes `ip6tables-restore`
  reject the file and leaves IPv6 unfiltered.

## Pentest suite

`tests/firewall/pentest.py` builds a network-namespace topology — an untrusted
`eth0` with an attacker behind it, a `tailscale0` with a tailnet peer, and a
`docker0` with a container — applies the rendered rulesets, and attacks the
host through the real kernel netfilter path. It covers reachability of open and
closed ports, source-address spoofing via raw sockets, container reachability
and egress, rate limiting, and fail-closed behaviour when the custom chain is
flushed.

```sh
unshare --user --map-root-user --net --mount --fork \
    python3 tests/firewall/pentest.py --repo-root .

cargo test --test firewall_pentest -- --include-ignored
```

Needs `iproute2`, `iptables` and `python3` with `jinja2`/`pyyaml`. It runs
entirely in namespaces, so it needs no VM, no container daemon and no real
root on the host.
