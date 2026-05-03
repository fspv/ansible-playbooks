use std::path::Path;

use similar::TextDiff;
use tracing::info;

pub mod apt_package;
pub mod apt_repo;
pub mod command;
pub mod directory;
pub mod download;
pub mod file;
pub mod line_in_file;
pub mod marker;
pub mod service;
pub mod symlink;
pub mod systemd_unit;
pub mod user;

/// Log a unified diff between the on-disk content (`current`) and the
/// desired content (`desired`) for `path`. Called by the `File` and
/// `LineInFile` backends whenever they detect a content change so the
/// operator can see exactly what would be (or was just) modified, in both
/// dry-run and apply mode. No diff is logged when nothing changed —
/// callers gate this behind their own drift check.
pub(crate) fn log_file_diff(path: &Path, current: &str, desired: &str) {
    let diff = TextDiff::from_lines(current, desired);
    let mut rendered = diff
        .unified_diff()
        .context_radius(3)
        .header("current", "desired")
        .to_string();
    if rendered.is_empty() {
        rendered.push_str("(no textual difference)\n");
    }
    info!(path = %path.display(), "file content drift\n{rendered}");
}
