use std::env;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum Error {
    NotFound { program: String },
    Io { command: String, source: io::Error },
    Timeout { command: String, after: Duration },
    NonZero { command: String, output: Output },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { program } => write!(f, "`{program}` not found on PATH"),
            Self::Io { command, source } => write!(f, "`{command}`: {source}"),
            Self::Timeout { command, after } => {
                write!(f, "`{command}` timed out after {after:?}")
            }
            Self::NonZero { command, output } => {
                write!(
                    f,
                    "`{command}` exited with {:?}\n--- stdout ---\n{}--- stderr ---\n{}",
                    output.status.code(),
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr),
                )
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Locate a program on `$PATH`, returning its absolute path.
///
/// # Errors
/// Returns [`Error::NotFound`] if no entry on `$PATH` contains an executable
/// file with the given name.
pub fn which(program: &str) -> Result<PathBuf, Error> {
    let path = env::var_os("PATH").ok_or_else(|| Error::NotFound {
        program: program.into(),
    })?;
    env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable_file(candidate))
        .ok_or_else(|| Error::NotFound {
            program: program.into(),
        })
}

/// Assert a program is on `$PATH`. Suitable for `#[test]` use.
///
/// # Panics
/// Panics if [`which`] cannot resolve `program`.
pub fn assert_on_path(program: &str) {
    match which(program) {
        Ok(path) => eprintln!("found {program} at {}", path.display()),
        Err(e) => panic!("{e}"),
    }
}

/// Run a command to completion; succeed only if the exit status is zero.
///
/// # Errors
/// Returns [`Error::Io`] on spawn failure, or [`Error::NonZero`] otherwise.
pub fn run_ok(program: &str, args: &[&str]) -> Result<Output, Error> {
    let output = build(program, args).output().map_err(|source| Error::Io {
        command: format_command(program, args),
        source,
    })?;
    check_status(program, args, output)
}

/// Run a command with a wall-clock timeout. The child is killed and reaped on
/// timeout.
///
/// # Errors
/// Returns [`Error::Io`] on spawn or wait failure, [`Error::Timeout`] if the
/// child outlives `timeout`, or [`Error::NonZero`] on a non-zero exit.
pub fn run_ok_timeout(program: &str, args: &[&str], timeout: Duration) -> Result<Output, Error> {
    let mut child = build(program, args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| Error::Io {
            command: format_command(program, args),
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
                return Err(Error::Timeout {
                    command: format_command(program, args),
                    after: timeout,
                });
            }
            Ok(None) => thread::sleep(poll),
            Err(source) => {
                return Err(Error::Io {
                    command: format_command(program, args),
                    source,
                });
            }
        }
    }

    let output = child.wait_with_output().map_err(|source| Error::Io {
        command: format_command(program, args),
        source,
    })?;
    check_status(program, args, output)
}

fn build(program: &str, args: &[&str]) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null());
    cmd
}

fn check_status(program: &str, args: &[&str], output: Output) -> Result<Output, Error> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(Error::NonZero {
            command: format_command(program, args),
            output,
        })
    }
}

fn format_command(program: &str, args: &[&str]) -> String {
    if args.is_empty() {
        program.to_owned()
    } else {
        format!("{program} {}", args.join(" "))
    }
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file())
}
