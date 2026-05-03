# host-setup architecture

A declarative, async, batching system-configuration framework that
replaces the ansible playbooks in this repo. The binary at `main.rs`
takes a per-host YAML config plus one or more `--bundle` selections, builds
an in-memory DAG of resources, and executes it concurrently with per-family
batching. Failures are surfaced loudly; no silent recovery.

## Module layout

```
src/
├── main.rs            CLI parsing + bundle dispatch
├── lib.rs             public module declarations
├── env.rs             host facts (container?, run mode, architecture)
├── config.rs          serde-deserialised per-host YAML
├── error.rs           Error + BackendError types
├── resource.rs        Resource trait, ResourceId, Skip, Changed, BatchFamily
├── plan.rs            Plan: ordered list of Resources with deps
├── executor.rs        topo-sort + per-family batching + concurrent dispatch
├── batcher.rs         Batcher trait (private — only built-in backends use it)
├── backends.rs        backend module declarations + log_file_diff helper
├── backends/          concrete Resource implementations
│   ├── apt_package.rs
│   ├── apt_repo.rs
│   ├── command.rs
│   ├── directory.rs
│   ├── download.rs
│   ├── file.rs
│   ├── line_in_file.rs
│   ├── marker.rs
│   ├── service.rs
│   ├── symlink.rs
│   ├── systemd_unit.rs
│   └── user.rs
└── bundles/           role-equivalent compositions
    ├── apt.rs
    ├── tailscale.rs
    ├── virtualbox.rs
    ├── ...
    └── common_devserver.rs   (top-level entry point)
```

The `bundles.rs` module owns `Context`, the threaded mutable state used by
every bundle's `build` function.

## Core data flow

```
       CLI flags                YAML config           target host
           │                         │                    │
           ▼                         ▼                    ▼
       Args::parse              Config::load          Env::detect
           │                         │                    │
           └──────────┬──────────────┴───────┬────────────┘
                      ▼                      ▼
                  bundles::Context (owns &mut Plan, &Env, &Config)
                      │
                      │  ctx.common_devserver() — call chain spreads
                      │  through every bundle's `build`, registering
                      │  resources on the Plan.
                      ▼
                  Plan (Vec<Node>, each carries deps via ResourceId)
                      │
                      ▼
                  Executor::run
                      │
                      │  topo-sort into levels
                      │  for each level:
                      │    group by BatchFamily
                      │    spawn per group via tokio::task::JoinSet
                      │    batched groups call Batcher::converge_batch
                      │    individual groups call Resource::converge_one
                      ▼
                  Report (Vec<ResourceOutcome>)
```

## Resource trait

Defined in `resource.rs`:

```rust
pub trait Resource: AsAny + Debug + Send + Sync + 'static {
    fn id_hint(&self) -> String;
    fn deps(&self) -> &[ResourceId] { &[] }
    fn skip_when(&self) -> &Skip { &Skip::Never }
    fn batch_family(&self) -> Option<BatchFamily> { None }
    async fn converge_one(&self, env: &Env) -> Result<Changed, BackendError>;
}
```

Every backend struct implements `Resource`. The `converge_one` method does
the full sense + diff + apply cycle in one call, returning one of:

- `Changed::Yes` — resource was (or, in dry-run, would have been) changed
- `Changed::No`  — resource is already in desired state
- `Changed::Skipped` — `skip_when` matched (`Skip::InContainer` etc.)

`AsAny` is a blanket trait that lets the executor downcast `&dyn Resource`
back to the concrete type inside a `Batcher`.

### Why `converge_one` instead of separate sense/diff/apply

Backends compose well as a single async function. The dry-run distinction
lives inside each backend — every one of them senses unconditionally and
gates the *write* path on `env.is_dry_run()`. This keeps the executor
ignorant of mode and means a dry-run still hits dpkg-query, getent,
HTTPS GETs, etc. — only the mutations skip.

## Backends

Each backend is a struct with named fields that callers construct via
struct literals (no fluent builders). All include `deps: Vec<ResourceId>`
and `skip_when: Skip` for cross-cutting concerns. Removal is a separate
backend (`AbsentFile`, `AbsentAptPackage`) rather than a flag on the
present-tense type — this keeps each struct's fields all-relevant.

`Service` has `restart_on: Vec<ResourceId>` and `Command` has
`trigger_on: Option<Vec<ResourceId>>`: ansible's notify-handler shape.
The executor records every `Changed::Yes` outcome in `Env`'s run-state
between levels; downstream resources read `env.any_changed(&[…])` to
decide whether their listed inputs drifted in the current run. A
`Service` whose `restart_on` ids changed runs `systemctl restart`; a
`Command` whose `trigger_on` is set and saw no change is a no-op. Use
this for "restart docker.service when daemon.json drifts",
"update-ca-certificates when /usr/local/share/ca-certificates/* drifts",
"udevadm reload-rules + trigger when udev rules change", etc.
`SystemdUnit` runs `systemctl daemon-reload` on its own change; bundle
authors don't have to wire that.

| Backend       | Senses via                              | Mutates via                         |
| ------------- | --------------------------------------- | ----------------------------------- |
| `AptPackage`  | `dpkg-query -W`                         | `apt-get update` + `install`        |
| `AbsentAptPackage` | `dpkg-query -W`                    | `apt-get remove --purge`            |
| `AptRepo`     | (file content)                          | atomic write to `/etc/apt/sources.list.d/<name>.list` |
| `Command`     | optional `trigger_on` change-check      | spawn argv when triggered (or always, if no `trigger_on`) |
| `Directory`   | `stat`, `getent passwd/group`           | `create_dir_all` + `chmod` + `chown` |
| `Download`    | HTTPS GET + byte-compare against disk   | atomic temp+rename                  |
| `File`        | content + permissions                   | atomic temp+rename                  |
| `AbsentFile`  | `stat`                                  | `remove_file`                       |
| `LineInFile`  | regex-match + content                   | atomic temp+rename                  |
| `Marker`      | (none, no-op)                           | (none, no-op)                       |
| `Service`     | `systemctl is-enabled` + `is-active` + `restart_on` change-check | `systemctl enable`/`start`/`restart` |
| `Symlink`     | `read_link`                             | `remove_file` + `symlink`           |
| `SystemdUnit` | content of `/etc/systemd/system/<name>` | atomic write + `systemctl daemon-reload` |
| `User`        | `getent passwd`, `id -nG`               | `useradd` / `usermod -aG` / `usermod -p` |

### Marker

A no-op resource used as an aggregation point: every bundle returns
exactly one `Marker` whose `deps` cover all the resources the bundle
added. Downstream consumers depend on the single `ResourceId` rather
than a `Vec<ResourceId>`. See `backends/marker.rs`.

### Batcher

Some backends share a `BatchFamily` (e.g. `BatchFamily::AptPackage`).
The executor groups resources by family at each topo level and dispatches
them to a registered `Batcher` (defined in `batcher.rs`, registered in
`Executor::new`) which combines the work — for apt that means a single
`apt-get update` followed by one `apt-get install ...` for every package
in the level. Resources without a family (Files, Commands, …) execute
individually but still concurrently within the level.

## Plan + Executor

`Plan` (in `plan.rs`) is a `Vec<Node>` where each node carries a
`ResourceId`, the `Arc<dyn Resource>`, and its `deps`. Plan is build-then-
execute: bundle code populates it; the executor consumes it.

`Executor::run` (in `executor.rs`):

1. **Topological sort** via Kahn's algorithm. Groups resources into
   levels where every node's deps lie at a strictly earlier level. Cycles
   surface as `Error::PlanCycle`; a dep pointing at a non-existent
   resource surfaces as `Error::PlanReferencesUnknownResource`.
2. **For each level, in order:**
   - Group resources by `batch_family()`.
   - For groups with a registered `Batcher`, spawn one task that calls
     `Batcher::converge_batch`.
   - For groups without one, spawn a task per resource calling
     `Resource::converge_one`.
   - Wait for the whole level's `JoinSet` to drain before moving on.
3. The first error in any task aborts the run and returns
   `Error::Backend { resource, source }`.

Concurrency boundaries: parallelism within a level, never across.
Cross-level ordering is the user-declared dep graph.

## Bundles + Context

A *bundle* is a composable unit roughly mapping to one ansible role.
Bundle files live under `src/bundles/` and expose:

```rust
pub fn build(ctx: &mut Context<'_>) -> ResourceId
```

The function adds resources to `ctx.plan`, returns the `ResourceId` of a
`Marker` named `<bundle>:ready`, and uses `ctx.<other>()` calls for any
upstream-bundle dependencies.

`Context` (in `bundles.rs`) is the mutable state threaded through every
`build`:

```rust
pub struct Context<'a> {
    pub plan: &'a mut Plan,
    pub env: &'a Env,
    pub config: &'a Config,
    cache: HashMap<&'static str, ResourceId>,
}
```

The cache memoises each bundle's result by string key — the first call to
`ctx.apt()` runs `apt::build` and stores the returned `ResourceId`; every
later call returns the cached id without re-registering anything. This
gives:

- **Implicit dep graph** — no separate "bundle X depends on bundle Y"
  declaration to keep in sync. The dep is the call.
- **Idempotent at-most-once** — calling `ctx.apt()` from five different
  bundles still produces one apt:ready marker.
- **Order-independent dispatch** — `main.rs` can call `ctx.users()` and
  `ctx.apt()` in any order; recursive `ctx.<other>()` calls inside each
  build resolve deps correctly via the cache.

`main.rs` parses CLI bundle names, looks each up in a `match`, and calls
the corresponding `ctx.<bundle>()`. Most users invoke
`--bundle common-devserver` (the default), which transitively pulls in
every bundle the legacy `common-devserver.yml` playbook would.

### Adding a new bundle

1. Create `src/bundles/<name>.rs` with `pub fn build(ctx: &mut Context<'_>) -> ResourceId`.
2. Add `pub mod <name>;` in `src/bundles.rs`.
3. Add a `pub fn <name>(&mut self) -> ResourceId { self.memoized("<name>", <name>::build) }` method on `Context`.
4. Add a `BundleName::<Name>` variant in `main.rs`, plus a dispatch arm.

Most bundles only need (1)–(3) if they're invoked transitively (via
`ctx.<name>()` from another bundle); (4) is required only if the user
should be able to pass `--bundle <name>` explicitly.

## Env

`env.rs` carries facts about the target host: `is_container()`,
`run_mode()`, `architecture()`, `ubuntu_codename()` (read from
`/etc/os-release`), `is_in_virtualbox()` (lspci probe). It is detected
once at startup and passed into every backend via
`Resource::converge_one(env: &Env, ...)`. `Env::detect()` returns
`Result` — a missing/unparseable `/etc/os-release` is a hard error so
bundles never bake a guessed codename into apt repo URLs.

`Skip::InContainer` is the conventional way to skip host-only resources
inside containers (e.g. systemd Service resources). Bundles that should
not run at all in containers gate at the bundle level by returning an
empty `Marker` early.

## Config

`config.rs` is the typed shape of the per-host YAML. Loaded once at
startup, passed through `Context`. Field shape mirrors the keys in
`manual/common.yml`. Unknown fields are silently ignored by serde, so
existing legacy configs parse without modification.

Currently modelled per-host fields: users (map), nvidia (bool),
system_vendor, ca_cert (map), iptables_open_ports (nested map),
apt_repos (list — closed enum of recognised archive components, defaults
to ubuntu/security/updates/backports; opt into ubuntu-proposed or
ppa-pv-safronov-backports by listing them). New fields go here as more
bundles need them.

## Errors

`error.rs` declares two types:

- `Error` — the top-level error returned by `Executor::run`. Variants
  for plan validation failures and a `Backend { resource, source }` wrapper
  carrying a `BackendError`.
- `BackendError` — an opaque struct holding `backend: &'static str`,
  `message: String`, and an optional boxed `source`. Backends construct
  via `BackendError::new` / `BackendError::with_source`. The structure is
  intentionally opaque (no per-backend enum variants escape) because no
  caller currently pattern-matches on backend failure modes; the only
  consumer is the operator reading the message.

## Logging

The binary configures `tracing-subscriber` with `EnvFilter`. Defaults to
`info,host_setup=debug`. Override via `RUST_LOG`. Notable events:

- `info!` — top-level run progress, change reports, executor batch
  dispatches, apt install batches.
- `info!("file content drift\n...")` — emitted from `File` and
  `LineInFile` backends whenever their planned write would differ from
  what's on disk. Body is a unified diff. Visible in both dry-run and
  apply mode so the operator sees exactly what would change.
- `debug!` — per-resource sense decisions ("file already in desired
  state", "user already exists", batched apt-package no-ops).

## Toolchain gates

Every change must pass, with no warnings:

```
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo build --tests
cargo test
```

The lint config in `Cargo.toml` denies `clippy::all`, `pedantic`, `nursery`,
`cargo`, plus `unwrap_used`, `expect_used`, `panic`, `print_*`, `todo`,
`unimplemented`, and `dbg_macro` outside tests. See `CLAUDE.md` for the
full project Rust style guide.

## Tests

Two flavours under `tests/`:

- **Default** — pure-function tests for backends that don't need root
  (file writes to tempdirs, plan-level executor logic, command spawning).
  These run on every `cargo test`.
- **Container e2e** — `#[ignore]`-tagged tests in `tests/user_e2e.rs` and
  `tests/command_backend.rs`-style suites that mutate `/etc/passwd` etc.
  Each one calls `assert_in_container()` so accidentally running with
  `--include-ignored` on a host fails loudly. The CI pipeline (`.github/
  workflows/host-setup.yml`) builds `Dockerfile.host-setup` which runs
  `cargo test --include-ignored` inside the container, exercising both
  suites in one shot.

## Design rules captured here so far

- Declarative DAG, not imperative. Bundles register resources; the
  executor decides the order.
- Struct literals at the call site, never fluent builders. Keeps every
  field discoverable from `User { ` IDE completion.
- No `pub use` re-exports in `lib.rs` / `bundles.rs`. Module path is the
  documentation: `host_setup::backends::file::File`.
- File contents and paths inline at the struct literal, never extracted
  into module-level `const`s. Reading top-to-bottom is more important
  than DRY.
- File modes are `fs::Permissions`, never bare `u32`.
- Fail loudly. No `warn!` then `continue`, no silent skip when a group
  doesn't exist, no swallowing subprocess errors. If the user's intent
  can't be carried out, return the error.
- Encapsulate shared concerns inside backends. `apt-get update` lives in
  the apt-package batcher, not in every bundle. `systemctl daemon-reload`
  lives in `SystemdUnit::converge_one`, not in every caller.
- Bundles return a single `Marker` `ResourceId` for downstream
  consumption. Never `Vec<ResourceId>` or a struct of ids.
- Bundle-to-bundle deps are expressed by calling `ctx.<other>()` inside
  `build`. The memoised `Context` ensures each bundle runs at most once.
- File and config paths byte-exact match the legacy ansible role they
  replace, so the framework can be applied on top of ansible-managed
  hosts without conflict.

The full list and rationale lives in the project's `CLAUDE.md` (Rust
section) plus per-rule memory files at
`/home/dev/.claude/projects/.../memory/`.
