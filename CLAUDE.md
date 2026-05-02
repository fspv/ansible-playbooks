# Project-specific instructions for Claude

## Ansible Role Structure

When creating new roles, follow this structure:

1. Don't create galaxy_info or meta files for roles. Use `meta/main.yml` only to specify dependencies between roles, for example:
    ```
    dependencies:
      - { role: pkgmanager }
      - { role: user }
    ```

2. **Task organization** - Structure tasks as follows:
   - `tasks/main.yml` should ONLY contain includes:
     ```yaml
     ---
     - include: packages.yml
     - include: configs.yml
     - include: services.yml
     ```
   - `tasks/packages.yml` - for package installation tasks
   - `tasks/configs.yml` - for configuration file tasks
   - `tasks/services.yml` - for service management tasks
   - Do not create empty files, if there is nothing to add there

3. **Static and dynamic files placement** - If files needs to be created on the disk during
   ansible run, always place this file to the templates director and add a .j2
   extension. This is needed in case in the future we will need to make this
   file dynamic, in which case we can just use jinja templating for the
   existing file.

4. **Example structure for a role:**
   ```
   roles/rolename/
   ├── tasks/
   │   ├── main.yml      (only includes)
   │   ├── packages.yml
   │   ├── configs.yml
   │   └── services.yml
   ├── meta/
   │   └── main.yml
   ├── handlers/
   │   └── main.yml
   └── templates/
       └── (template files as needed)
   ```

## Other Conventions

- When creating udev rules, follow the pattern from existing roles (e.g., yubico role)
- Use handlers for reloading services or triggering system updates
- Use correct yaml. For example, instead of `become: yes` use `become: true`
- When naming tasks use very short description with the task type prefix. For example when you want to create `/etc/nginx/nginx.conf` name the task `config nginx`
- ansible.builtin.include has been removed. Use include_tasks or import_tasks instead.
- Use "apt" to install packages. Don't use "package", because these recipies will only be used on Ubuntu

# Rust

Guidance for agents (and humans) writing Rust in this repo. Read this before
making changes; the bar is "code a senior Rust engineer would sign off on".

## Toolchain gates

Every change must pass, with no warnings:

```
cargo fmt --check
cargo clippy --tests --all-targets -- -D warnings
cargo build --tests
cargo test
```

The lint config in `Cargo.toml` already denies `clippy::all`, `pedantic`,
`nursery`, and `cargo`. Do not relax it. Do not paper over a lint with
`#[allow(...)]` unless you can defend the exemption in code review.

## Design rules

1. **Public API is a contract.** `pub` items get doc comments with `# Errors`
   and `# Panics` sections where applicable. Anything not in the public API
   stays private — keep the surface small.
2. **Errors are typed.** Use a single crate-level `Error` enum with named
   fields. Implement `Display` and `std::error::Error::source()`. Never
   stringify an error you could carry structurally.
3. **No `unwrap` in library code.** `expect`, `panic`, `print_*` are allowed
   in `#[test]` only (already configured). In tests, panic with the formatted
   error (`unwrap_or_else(|e| panic!("{e}"))`) so failures are useful.
4. **Don't shell out for things `std` can do.** Walk `$PATH` with
   `env::split_paths`, don't spawn `sh -c command -v`. Every subprocess is a
   portability and performance liability.
5. **Dependencies are fine when they earn their keep.** Reach for widely
   used, well-maintained crates (`reqwest`, `tokio`, `serde`, `clap`, …)
   instead of reinventing them or shelling out to a CLI tool that happens to
   do the same thing. Don't pull in obscure single-maintainer crates without
   thinking about it, and don't add a dep just to save three lines.
6. **Borrow, don't clone.** Take `&str`/`&Path`/`&[T]` in arguments. Convert
   to owned types only at the boundary that needs to store them.
7. **Newtype where it pays.** A `Duration` is fine; a bare `u64` for "seconds"
   is not. Don't invent newtypes for types that already carry their meaning.
8. **`const` over `let` for fixed values.** Image refs, timeouts, and
   thresholds belong at module top as `const`, not buried in call sites.

## Style rules

1. **Self-documenting names beat comments.** A reader should know what a
   function does from its name without opening the body. Don't be afraid of
   long names: `assert_systemd_unit_is_enabled`, `run_command_must_succeed`,
   `read_symlink_target`, `format_command_invocation`. Banned: vague verbs
   like `check_status` (status of *what*?), `read_text` (from *where*?),
   `home`, `process` — rename until the answer is in the name. This applies
   to private helpers and `Error` variants too, not just the public API.
2. **Helpers belong in `lib.rs`, not duplicated across test files.** If the
   same shape of helper appears in two test files — or even looks like it
   *might* — promote it to `lib.rs` with a descriptive name and a doc
   comment. Test files should be a flat list of `#[test]` fns plus their
   call sites, not a grab-bag of local utilities.
3. **Module-level `//!` docs only when they carry *why*.** Don't restate
   what the file is from its name and contents. A `//!` block is justified
   when it explains a security contract, an invariant, or a constraint that
   isn't visible from the test names — otherwise delete it.
4. **Comments only for non-obvious *why*.** Hidden constraints, surprising
   invariants, workarounds for specific bugs. No "// run command" lines, no
   restating the function signature in prose. Section headers explaining
   *what* a group of tests does are also banned — the test names already
   say it.
5. **No headers, no banners, no decorative dividers** in source — only in
   test files where `// ---------- group ----------` actually helps a human
   skim 25+ `#[test]` functions. A handful of tests does not qualify; don't
   add dividers preemptively.
6. **No `cfg` gates for platforms this code never runs on.** This crate
   runs on Linux only — don't write `#[cfg(unix)]` / `#[cfg(not(unix))]`
   alternates "for portability". `use std::os::unix::*` directly.
7. **Imports grouped: `std`, external crates, local.** Blank line between
   groups. `cargo fmt` handles within-group sorting.
8. **No `mod.rs` ceremony or premature module splits.** One file is fine
   until it isn't. Split when a coherent submodule emerges, not before.
9. **Prefer iterator chains to imperative loops** when the chain stays under
   ~4 combinators and reads top-to-bottom. Past that, a `for` loop with named
   bindings is clearer.
10. **Match exhaustively, no `_ =>` catch-alls** on internal enums — let the
    compiler tell you when a variant is added.

## Testing rules

1. Tests live under `tests/` (integration) unless the unit needs private
   access. Name files by feature area (`binaries.rs`, `podman.rs`).
2. One assertion theme per `#[test]`. If a test needs a paragraph of comments
   to explain what it's verifying, split it.
3. Test failure messages must include the offending value. `assert!(x.contains(y), "missing y in: {x}")`.
4. Avoid sleeps. If you need to wait for a process, use `try_wait` against a
   deadline (see `run_command_must_succeed_within`), not `sleep(big_number)`.
5. Don't test the standard library. Test the contract this crate offers.
6. **Probe contracts directly, not through proxies.** To check that a unix
   socket is unreachable, `UnixStream::connect(path)` and assert
   `ErrorKind::PermissionDenied`. Don't shell out to `curl --unix-socket`
   and grep stderr — the keyword-matching is brittle and the error category
   is already structured. Same applies elsewhere: a `reqwest` GET beats
   piping `curl`'s stdout (which can deadlock on full pipe buffers when run
   under `run_command_must_succeed_within`).

## Anti-patterns to reject

- `Box<dyn Error>` in library return types — use the concrete enum.
- `String` parameters where `&str` works.
- `clone()` to silence the borrow checker without understanding why.
- `unwrap()` "because it can't fail here" — if it truly can't, prove it with
  the type system; if it can, return an error.
- `unsafe` — forbidden by `Cargo.toml`. Don't try.
- Wrapping every function body in `Result<_, Box<dyn Error + Send + Sync>>`
  when the failure modes are knowable and few.
- Defensive checks for things the type system already guarantees.
- Vague identifiers: `read_text`, `check_status`, `home`, `data`, `do_thing`,
  `Error::NotFound` (not found *of what*?). Rename to something that answers
  the obvious follow-up question.
- Per-test-file local helpers that wrap a call to `lib.rs`. If the wrapper
  is worth writing, it's worth promoting; if it's not, inline the call.
- Module-level `//!` docs that describe what a file contains rather than
  why it exists.
- `#[cfg(unix)]` / `#[cfg(not(unix))]` shims for code that will only ever
  run on Linux.

## When in doubt

Look at `src/lib.rs`. It's the reference for tone, error shape, helper
granularity, and doc style in this crate. New code should feel like it was
written by the same person.
