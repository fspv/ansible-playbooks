use std::env;
use std::fmt;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const PROMETHEUS_METRICS_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug)]
pub enum Error {
    ProgramNotFoundOnPath { program: String },
    CommandSpawnOrWaitFailed { command: String, source: io::Error },
    CommandExceededTimeout { command: String, after: Duration },
    CommandExitedNonZero { command: String, output: Output },
    CommandUnexpectedlySucceeded { command: String, output: Output },
    FileReadFailed { path: PathBuf, source: io::Error },
    SymlinkReadFailed { path: PathBuf, source: io::Error },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProgramNotFoundOnPath { program } => {
                write!(f, "`{program}` not found on PATH")
            }
            Self::CommandSpawnOrWaitFailed { command, source } => {
                write!(f, "`{command}`: {source}")
            }
            Self::CommandExceededTimeout { command, after } => {
                write!(f, "`{command}` timed out after {after:?}")
            }
            Self::CommandExitedNonZero { command, output } => {
                write!(
                    f,
                    "`{command}` exited with {:?}\n--- stdout ---\n{}--- stderr ---\n{}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                )
            }
            Self::CommandUnexpectedlySucceeded { command, output } => {
                write!(
                    f,
                    "`{command}` was expected to fail but exited 0\n--- stdout ---\n{}--- stderr ---\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                )
            }
            Self::FileReadFailed { path, source } => {
                write!(f, "read `{}`: {source}", path.display())
            }
            Self::SymlinkReadFailed { path, source } => {
                write!(f, "readlink `{}`: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CommandSpawnOrWaitFailed { source, .. }
            | Self::FileReadFailed { source, .. }
            | Self::SymlinkReadFailed { source, .. } => Some(source),
            Self::ProgramNotFoundOnPath { .. }
            | Self::CommandExceededTimeout { .. }
            | Self::CommandExitedNonZero { .. }
            | Self::CommandUnexpectedlySucceeded { .. } => None,
        }
    }
}

/// Locate a program on `$PATH`, returning its absolute path.
///
/// # Errors
/// Returns [`Error::ProgramNotFoundOnPath`] if no entry on `$PATH` contains
/// an executable file with the given name.
pub fn find_program_on_path(program: &str) -> Result<PathBuf, Error> {
    let path = env::var_os("PATH").ok_or_else(|| Error::ProgramNotFoundOnPath {
        program: program.into(),
    })?;
    env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable_file(candidate))
        .ok_or_else(|| Error::ProgramNotFoundOnPath {
            program: program.into(),
        })
}

/// Assert a program is on `$PATH`. Suitable for `#[test]` use.
///
/// # Panics
/// Panics if [`find_program_on_path`] cannot resolve `program`.
pub fn assert_program_is_on_path(program: &str) {
    match find_program_on_path(program) {
        Ok(path) => eprintln!("found {program} at {}", path.display()),
        Err(e) => panic!("{e}"),
    }
}

/// Run a command to completion; succeed only if the exit status is zero.
///
/// # Errors
/// Returns [`Error::CommandSpawnOrWaitFailed`] on spawn failure, or
/// [`Error::CommandExitedNonZero`] otherwise.
pub fn run_command_must_succeed(program: &str, args: &[&str]) -> Result<Output, Error> {
    let output = build_command_with_null_stdin(program, args)
        .output()
        .map_err(|source| Error::CommandSpawnOrWaitFailed {
            command: format_command_invocation(program, args),
            source,
        })?;
    require_zero_exit_status(program, args, output)
}

/// Run a command with a wall-clock timeout. The child is killed and reaped
/// on timeout.
///
/// # Errors
/// Returns [`Error::CommandSpawnOrWaitFailed`] on spawn or wait failure,
/// [`Error::CommandExceededTimeout`] if the child outlives `timeout`, or
/// [`Error::CommandExitedNonZero`] on a non-zero exit.
pub fn run_command_must_succeed_within(
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<Output, Error> {
    let mut child = build_command_with_null_stdin(program, args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::CommandSpawnOrWaitFailed {
            command: format_command_invocation(program, args),
            source,
        })?;

    let deadline = Instant::now() + timeout;
    let poll = Duration::from_millis(100);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::CommandExceededTimeout {
                    command: format_command_invocation(program, args),
                    after: timeout,
                });
            }
            Ok(None) => thread::sleep(poll),
            Err(source) => {
                return Err(Error::CommandSpawnOrWaitFailed {
                    command: format_command_invocation(program, args),
                    source,
                });
            }
        }
    }

    let output = child
        .wait_with_output()
        .map_err(|source| Error::CommandSpawnOrWaitFailed {
            command: format_command_invocation(program, args),
            source,
        })?;
    require_zero_exit_status(program, args, output)
}

/// Run a command and require a non-zero exit. Mirrors
/// [`run_command_must_succeed`] for negative privilege checks: succeed only
/// when the command failed.
///
/// # Errors
/// Returns [`Error::CommandSpawnOrWaitFailed`] on spawn failure, or
/// [`Error::CommandUnexpectedlySucceeded`] when the command exited 0.
pub fn run_command_must_fail(program: &str, args: &[&str]) -> Result<Output, Error> {
    let output = build_command_with_null_stdin(program, args)
        .output()
        .map_err(|source| Error::CommandSpawnOrWaitFailed {
            command: format_command_invocation(program, args),
            source,
        })?;
    if output.status.success() {
        Err(Error::CommandUnexpectedlySucceeded {
            command: format_command_invocation(program, args),
            output,
        })
    } else {
        Ok(output)
    }
}

/// Read the contents of a file at `path` into a `String`, carrying the path
/// in any error.
///
/// # Errors
/// Returns [`Error::FileReadFailed`] if the file is missing, unreadable, or
/// not valid UTF-8.
pub fn read_file_to_string(path: &Path) -> Result<String, Error> {
    std::fs::read_to_string(path).map_err(|source| Error::FileReadFailed {
        path: path.to_path_buf(),
        source,
    })
}

/// Resolve a symlink at `path` one level, returning its target.
///
/// # Errors
/// Returns [`Error::SymlinkReadFailed`] if the path is not a symlink or
/// cannot be read.
pub fn read_symlink_target(path: &Path) -> Result<PathBuf, Error> {
    std::fs::read_link(path).map_err(|source| Error::SymlinkReadFailed {
        path: path.to_path_buf(),
        source,
    })
}

fn build_command_with_null_stdin(program: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null());
    cmd
}

fn require_zero_exit_status(program: &str, args: &[&str], output: Output) -> Result<Output, Error> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(Error::CommandExitedNonZero {
            command: format_command_invocation(program, args),
            output,
        })
    }
}

fn format_command_invocation(program: &str, args: &[&str]) -> String {
    if args.is_empty() {
        program.to_owned()
    } else {
        format!("{program} {}", args.join(" "))
    }
}

fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Home directory of the user running the test process, from `$HOME`.
///
/// # Panics
/// Panics if `$HOME` is unset.
#[must_use]
pub fn current_user_home_dir() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_else(|| panic!("HOME unset")))
}

/// Group names the user running the test process belongs to (primary plus
/// supplementary), via `id -nG`.
///
/// # Panics
/// Panics if `id -nG` cannot be spawned or exits non-zero.
#[must_use]
pub fn current_user_group_names() -> Vec<String> {
    let out = run_command_must_succeed("id", &["-nG"]).unwrap_or_else(|e| panic!("{e}"));
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// Assert the current user is not a member of `group`. Use to guard against
/// roles silently granting root-equivalent access (e.g. `docker`, `libvirt`).
///
/// # Panics
/// Panics if the user is in `group`.
pub fn assert_current_user_not_in_group(group: &str) {
    let groups = current_user_group_names();
    assert!(
        !groups.iter().any(|g| g == group),
        "user is unexpectedly in group `{group}`; groups: {groups:?}"
    );
}

/// Assert a systemd unit reports an enabled-ish state from `systemctl
/// is-enabled` (`enabled`, `static`, `alias`, or `enabled-runtime`).
///
/// # Panics
/// Panics if `systemctl is-enabled` fails or reports a different state.
pub fn assert_systemd_unit_is_enabled(unit: &str) {
    let out = run_command_must_succeed("systemctl", &["is-enabled", unit])
        .unwrap_or_else(|e| panic!("{e}"));
    let state = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    assert!(
        matches!(
            state.as_str(),
            "enabled" | "static" | "alias" | "enabled-runtime"
        ),
        "{unit} is-enabled => {state}"
    );
}

/// Assert a systemd unit is currently active per `systemctl is-active`.
///
/// # Panics
/// Panics if `systemctl is-active` does not report `active`.
pub fn assert_systemd_unit_is_active(unit: &str) {
    let out = run_command_must_succeed("systemctl", &["is-active", unit])
        .unwrap_or_else(|e| panic!("{e}"));
    let state = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    assert_eq!(state, "active", "{unit} is-active => {state}");
}

/// GET `url` and assert the body parses as a Prometheus text exposition
/// (first line is `# HELP` or `# TYPE`).
///
/// # Panics
/// Panics if the request fails, returns a non-2xx status, or the body does
/// not look like a Prometheus exposition.
pub fn assert_url_returns_prometheus_metrics(url: &str) {
    let client = reqwest::blocking::Client::builder()
        .timeout(PROMETHEUS_METRICS_REQUEST_TIMEOUT)
        .build()
        .unwrap_or_else(|e| panic!("build client: {e}"));

    let resp = client
        .get(url)
        .send()
        .unwrap_or_else(|e| panic!("GET {url}: {e}"));
    let status = resp.status();
    assert!(status.is_success(), "GET {url} -> {status}");

    let body = resp
        .text()
        .unwrap_or_else(|e| panic!("read body from {url}: {e}"));
    let first = body.lines().next().unwrap_or("");
    assert!(
        first.starts_with("# HELP") || first.starts_with("# TYPE"),
        "{url} did not return a Prometheus metrics body; first line: {first:?}"
    );
}
