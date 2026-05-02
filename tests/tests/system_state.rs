use std::path::Path;

use ansible_playbook_tests::{read_file_to_string, read_symlink_target, run_command_must_succeed};

#[test]
fn inotify_max_user_instances_raised() {
    let v = read_file_to_string(Path::new("/proc/sys/fs/inotify/max_user_instances"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(v.trim(), "8192", "unexpected value: {v:?}");
}

#[test]
fn ping_group_range_widened() {
    let v = read_file_to_string(Path::new("/proc/sys/net/ipv4/ping_group_range"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(v.split_whitespace().collect::<Vec<_>>(), ["1000", "10000"]);
}

#[test]
fn user_sysctl_drop_in_present() {
    let body = read_file_to_string(Path::new("/etc/sysctl.d/99-user.conf"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        body.contains("fs.inotify.max_user_instances=8192"),
        "missing inotify line in: {body}"
    );
    assert!(
        body.contains("net.ipv4.ping_group_range=1000 10000"),
        "missing ping_group_range line in: {body}"
    );
}

#[test]
fn bin_sh_points_at_bash() {
    let target = read_symlink_target(Path::new("/bin/sh")).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(target, Path::new("/bin/bash"));
}

#[test]
fn profile_d_history_present() {
    let body = read_file_to_string(Path::new("/etc/profile.d/history.sh"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        body.contains("PROMPT_COMMAND"),
        "history.sh does not set PROMPT_COMMAND: {body}"
    );
}

#[test]
fn no_vte_profile_d_scripts() {
    let entries =
        std::fs::read_dir("/etc/profile.d").unwrap_or_else(|e| panic!("read /etc/profile.d: {e}"));
    let leftovers: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            n.starts_with("vte")
                && std::path::Path::new(n)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("sh"))
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "vte scripts left behind: {leftovers:?}"
    );
}

#[test]
fn etc_localtime_is_zoneinfo_symlink() {
    let target = read_symlink_target(Path::new("/etc/localtime")).unwrap_or_else(|e| panic!("{e}"));
    let s = target.to_string_lossy();
    assert!(
        s.contains("/usr/share/zoneinfo/"),
        "/etc/localtime points at {s}, not a zoneinfo entry"
    );
}

#[test]
fn etc_timezone_matches_localtime() {
    let zone = read_file_to_string(Path::new("/etc/timezone"))
        .unwrap_or_else(|e| panic!("{e}"))
        .trim()
        .to_owned();
    let target = read_symlink_target(Path::new("/etc/localtime")).unwrap_or_else(|e| panic!("{e}"));
    let s = target.to_string_lossy();
    assert!(
        s.ends_with(&zone),
        "/etc/localtime ({s}) does not end with /etc/timezone value ({zone})"
    );
}

#[test]
fn timedatectl_reports_same_zone() {
    let out = run_command_must_succeed("timedatectl", &["show", "-p", "Timezone", "--value"])
        .unwrap_or_else(|e| panic!("{e}"));
    let reported = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let expected = read_file_to_string(Path::new("/etc/timezone"))
        .unwrap_or_else(|e| panic!("{e}"))
        .trim()
        .to_owned();
    assert_eq!(reported, expected);
}

#[test]
fn requested_locales_generated() {
    let conf = read_file_to_string(Path::new("/etc/locale.gen")).unwrap_or_else(|e| panic!("{e}"));
    let requested: Vec<&str> = conf
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    assert!(
        !requested.is_empty(),
        "no uncommented entries in locale.gen"
    );

    let out = run_command_must_succeed("locale", &["-a"]).unwrap_or_else(|e| panic!("{e}"));
    let available = String::from_utf8_lossy(&out.stdout).to_lowercase();
    for want in requested {
        let needle = want.replace('-', "").to_lowercase();
        assert!(
            available.contains(&needle),
            "locale `{want}` (normalised `{needle}`) missing from `locale -a`"
        );
    }
}

#[test]
fn journald_storage_capped() {
    let body = read_file_to_string(Path::new("/etc/systemd/journald.conf"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        body.contains("Storage=volatile"),
        "missing Storage=volatile in: {body}"
    );
    assert!(
        body.contains("SystemMaxUse=10M"),
        "missing SystemMaxUse=10M in: {body}"
    );
}
