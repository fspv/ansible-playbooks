use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::apt_package::AptPackage;
use crate::backends::command::Command;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::backends::symlink::Symlink;
use crate::resource::{ResourceId, Skip};

use super::Context;

// Mirrors roles/common-tweaks. Pulls in the various deps the legacy role's
// meta declared, then adds its own tasks: install ubuntu-drivers-common,
// symlink /bin/sh -> /bin/bash (the dash-as-sh workaround the role
// documents), write /etc/profile.d/history.sh for syslog'd bash history,
// drop /etc/sysctl.d/99-user.conf (and reload sysctls), remove the legacy
// command-not-found package, remove the vte profile.d shim, and run
// `ubuntu-drivers install`. Kernel-headers install is a TODO — Env doesn't
// expose the running kernel yet.

// Body length is data, not logic — most lines are the inlined history.sh
// content per project convention.
#[allow(clippy::too_many_lines)]
pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let apt_ready = ctx.apt();
    let users_ready = ctx.users();
    let tzdata_ready = ctx.tzdata();
    let ca_cert_ready = ctx.ca_cert();
    let chrony_ready = ctx.chrony();
    let gpg_ready = ctx.gpg();
    let locales_ready = ctx.locales();
    let apparmor_ready = ctx.apparmor();
    // Conditional bundles per the legacy common-tweaks meta. Each bundle
    // self-gates: nvidia checks ctx.config.nvidia, tuxedo checks
    // ctx.config.system_vendor == "TUXEDO", apc_ups and iptables skip in
    // containers. When a gate is off, the bundle returns an empty marker
    // so the dep edge is harmless.
    let nvidia_ready = ctx.nvidia();
    let tuxedo_ready = ctx.tuxedo();
    let apc_ups_ready = ctx.apc_ups();
    let iptables_ready = ctx.iptables();

    let drivers = ctx.plan.add(AptPackage {
        name: "ubuntu-drivers-common".to_string(),
        deps: vec![apt_ready],
        ..Default::default()
    });

    // Hardcode dash-as-sh -> bash. The legacy role's comment cites
    // https://wiki.ubuntu.com/DashAsBinSh and "too many bugs because of
    // dash as /bin/sh" as justification.
    let sh_link = ctx.plan.add(Symlink {
        path: PathBuf::from("/bin/sh"),
        target: PathBuf::from("/bin/bash"),
        deps: vec![apt_ready],
        ..Default::default()
    });

    let history = ctx.plan.add(File {
        path: PathBuf::from("/etc/profile.d/history.sh"),
        content: r#"if [[ -n "$BASH_VERSION" ]]; then
    # HIST* variables not really working for regular users
    # but will be useful for root
    # Ignore duplicates in .bash_history
    export HISTCONTROL=ignoredups 2>/dev/null
    # The  maximum  number of lines contained in the history file.
    export HISTFILESIZE=99999 2>/dev/null
    # Controls output of `history` command end enables time logging in .bash_history
    export HISTTIMEFORMAT="%a, %d %b %Y %T %z " 2>/dev/null

    declare LOG2SYSLOG_PREV_COMMAND
    declare LOG2SYSLOG_FIRST_RUN="yes"

    function log2syslog {
        # Append to histfile
        history -a

        # Get last command
        declare COMMAND
        LAST_HISTORY_UNFORMATTED=$(HISTTIMEFORMAT="" history 1 | awk '{ $1=""; print }')
        # Remove whitespace from the beginning
        # shellcheck disable=SC2116
        # shellcheck disable=SC2086
        COMMAND="$(echo ${LAST_HISTORY_UNFORMATTED})"

        # Return if there is nothing to log
        test "$COMMAND" = "" && return

        # Test if command hasn't been already written to log
        if ! test "$COMMAND" = "$LOG2SYSLOG_PREV_COMMAND"
        then
            if test "$LOG2SYSLOG_FIRST_RUN" = "yes"
            then
                LOG2SYSLOG_FIRST_RUN="no"
                readonly LOG2SYSLOG_FIRST_RUN
            else
                logger -p local1.notice -t bashhistory -i -- \
                    "${USER}[$$]:$(pwd):$SSH_CONNECTION:${COMMAND}"
            fi
        fi

        LOG2SYSLOG_PREV_COMMAND=$COMMAND
    }

    # shellcheck disable=SC2034
    readonly log2syslog

    export -f log2syslog

    # Trap executes before and after function.
    # So if trap successfully enoked it does all the magick right after
    # command has been started.
    trap log2syslog DEBUG

    # Trap has one security issue: it can be redefined

    # So here we are using readonly PROMPT_COMMAND to write something to log
    # even if some bad guy removed trap
    export PROMPT_COMMAND='log2syslog' 2>/dev/null
    readonly PROMPT_COMMAND
fi
"#
        .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    // Legacy role used `with_fileglob: /etc/profile.d/vte*.sh`; we don't have
    // glob support, so target the canonical filename. If other vte-* variants
    // appear (e.g. vte.sh proper, or future versions), add explicit
    // File-absent resources for each.
    let vte_absent = ctx.plan.add(File {
        path: PathBuf::from("/etc/profile.d/vte-2.91.sh"),
        absent: true,
        deps: vec![apt_ready],
        ..Default::default()
    });

    let command_not_found_absent = ctx.plan.add(AptPackage {
        name: "command-not-found".to_string(),
        absent: true,
        deps: vec![apt_ready],
        ..Default::default()
    });

    let sysctl_file = ctx.plan.add(File {
        path: PathBuf::from("/etc/sysctl.d/99-user.conf"),
        content: "fs.inotify.max_user_instances=8192\nnet.ipv4.ping_group_range=1000 10000\n"
            .to_string(),
        mode: Some(Permissions::from_mode(0o644)),
        deps: vec![apt_ready],
        ..Default::default()
    });

    // `sysctl --system` is idempotent; Command always re-runs but the result
    // converges. Skipped in containers — sysctl namespaces aren't writable.
    let sysctl_reload = ctx.plan.add(Command {
        name: "sysctl --system".to_string(),
        argv: vec!["sysctl".to_string(), "--system".to_string()],
        deps: vec![sysctl_file],
        skip_when: Skip::InContainer,
    });

    let drivers_install = ctx.plan.add(Command {
        name: "ubuntu-drivers install".to_string(),
        argv: vec![
            "sh".to_string(),
            "-c".to_string(),
            "DEBIAN_FRONTEND=noninteractive ubuntu-drivers install".to_string(),
        ],
        deps: vec![drivers],
        skip_when: Skip::InContainer,
    });

    // TODO: install linux-headers-{ansible_kernel} once Env exposes the
    // running kernel release. The legacy role pulls in matching headers so
    // nvidia DKMS modules build against the booted kernel.

    ctx.plan.add(Marker {
        name: "common_tweaks:ready".to_string(),
        deps: vec![
            users_ready,
            tzdata_ready,
            ca_cert_ready,
            chrony_ready,
            gpg_ready,
            locales_ready,
            apparmor_ready,
            nvidia_ready,
            tuxedo_ready,
            apc_ups_ready,
            iptables_ready,
            drivers,
            sh_link,
            history,
            vte_absent,
            command_not_found_absent,
            sysctl_file,
            sysctl_reload,
            drivers_install,
        ],
        ..Default::default()
    })
}
