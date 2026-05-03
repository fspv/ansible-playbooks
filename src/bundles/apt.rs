use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use crate::backends::absent_file::AbsentFile;
use crate::backends::apt_package::AptPackage;
use crate::backends::command::Command;
use crate::backends::file::File;
use crate::backends::marker::Marker;
use crate::config::AptRepo;
use crate::resource::ResourceId;

use super::Context;

// Paths and bodies are copied verbatim from the legacy ansible role's
// templates under roles/apt/templates/etc/apt/. They must stay byte-exact so
// this bundle can be applied on top of hosts already provisioned by the
// ansible playbook.
//
// Skipped vs the legacy role:
//  * /var/cache/apt and /var/lib/apt/lists wipes — those are run-time cache
//    state, not desired-state config. Re-deleting them on every converge
//    would force apt-get update to redownload package indexes.

// Body length is data, not logic — the apt config files and Ubuntu archive
// list templates are inlined verbatim per project convention.
#[allow(clippy::too_many_lines)]
pub fn build(ctx: &mut Context<'_>) -> ResourceId {
    let mode_644 = || Some(Permissions::from_mode(0o644));
    let codename = ctx.env.ubuntu_codename();

    let confd_files = vec![
        ctx.plan.add(File {
            path: PathBuf::from("/etc/apt/apt.conf.d/00noinstallrecommends"),
            content: "APT::Get::Install-Recommends \"false\";\n\
                      APT::Get::Install-Suggests \"false\";\n"
                .to_string(),
            mode: mode_644(),
            ..Default::default()
        }),
        ctx.plan.add(File {
            path: PathBuf::from("/etc/apt/apt.conf.d/30allowunauth"),
            content: "APT::Get::AllowUnauthenticated \"false\";\n".to_string(),
            mode: mode_644(),
            ..Default::default()
        }),
        ctx.plan.add(File {
            path: PathBuf::from("/etc/apt/apt.conf.d/00timeout"),
            content: "Acquire::http::Timeout \"3\";\n\
                      Acquire::ftp::Timeout \"3\";\n"
                .to_string(),
            mode: mode_644(),
            ..Default::default()
        }),
        ctx.plan.add(File {
            path: PathBuf::from("/etc/apt/apt.conf.d/10periodic"),
            content: "APT::Periodic::Update-Package-Lists \"0\";\n\
                      APT::Periodic::Download-Upgradeable-Packages \"0\";\n\
                      APT::Periodic::AutocleanInterval \"0\";\n\
                      APT::Periodic::Unattended-Upgrade \"0\";\n"
                .to_string(),
            mode: mode_644(),
            ..Default::default()
        }),
        // 00release is fully commented out — the role's template warns
        // against uncommenting APT::Default-Release. Kept byte-identical to
        // the legacy template, including the codename in the example line.
        ctx.plan.add(File {
            path: PathBuf::from("/etc/apt/apt.conf.d/00release"),
            content: format!(
                "# Please do not uncomment this. It is setting all packages with release xenial\n\
                 # to priority 990 and it is really bad\n\
                 #APT::Default-Release \"{codename}\";\n",
            ),
            mode: mode_644(),
            ..Default::default()
        }),
        // 99debug is a stash of commented Debug::* keys — exists so an
        // operator can uncomment to debug. Kept byte-identical.
        ctx.plan.add(File {
            path: PathBuf::from("/etc/apt/apt.conf.d/99debug"),
            content: "#Debug::Acquire::cdrom \"yes\";\n\
                      #Debug::Acquire::ftp \"yes\";\n\
                      #Debug::Acquire::gpgv \"yes\";\n\
                      #Debug::Acquire::http \"yes\";\n\
                      #Debug::Acquire::https \"yes\";\n\
                      #Debug::aptcdrom \"yes\";\n\
                      #Debug::BuildDeps \"yes\";\n\
                      #Debug::Hashes \"yes\";\n\
                      #Debug::IdentCdrom \"yes\";\n\
                      #Debug::IdentCDROM \"yes\";\n\
                      #Debug::NoLocking \"yes\";\n\
                      #Debug::pkgAcquire \"yes\";\n\
                      #Debug::pkgAcquire::Auth \"yes\";\n\
                      #Debug::pkgAcquire::Diffs \"yes\";\n\
                      #Debug::pkgAcquire::RRed \"yes\";\n\
                      #Debug::pkgAcquire::Worker \"yes\";\n\
                      #Debug::pkgAutoRemove \"yes\";\n\
                      #Debug::pkgDepCache::AutoInstall \"yes\";\n\
                      #Debug::pkgDepCache::Marker \"yes\";\n\
                      #Debug::pkgDPkgPM \"yes\";\n\
                      #Debug::pkgDPkgProgressReporting \"yes\";\n\
                      #Debug::pkgOrderList \"yes\";\n\
                      #Debug::pkgPackageManager \"yes\";\n\
                      #Debug::pkgPolicy \"yes\";\n\
                      #Debug::pkgProblemResolver \"yes\";\n\
                      #Debug::pkgProblemResolver::ShowScores \"yes\";\n\
                      #Debug::RunScripts \"yes\";\n\
                      #Debug::sourceList \"yes\";\n"
                .to_string(),
            mode: mode_644(),
            ..Default::default()
        }),
    ];

    // Notifier / motd cleanups the role removes so apt updates don't trigger
    // unattended-upgrades, MOTD scrapes, or daily/weekly cron noise.
    let cleanup_files: Vec<ResourceId> = [
        "/etc/update-motd.d/90-updates-available",
        "/etc/apt/apt.conf.d/99update-notifier",
        "/etc/cron.daily/update-notifier-common",
        "/etc/cron.weekly/update-notifier-common",
        // Ubuntu 24.04 ships sources in deb822 ubuntu.sources; the role
        // wipes it because we drive sources via /etc/apt/sources.list.d/*.list.
        "/etc/apt/sources.list.d/ubuntu.sources",
    ]
    .iter()
    .map(|p| {
        ctx.plan.add(AbsentFile {
            path: PathBuf::from(*p),
            ..Default::default()
        })
    })
    .collect();

    // /etc/apt/sources.list — kept as a placeholder pointing operators at
    // sources.list.d, byte-identical to the role's template.
    let sources_list = ctx.plan.add(File {
        path: PathBuf::from("/etc/apt/sources.list"),
        content: "# This file must be empty.\n\
                  # Please add new repositories to /etc/apt/sources.list.d/*.list\n"
            .to_string(),
        mode: mode_644(),
        ..Default::default()
    });

    let ppa_pin = ctx.plan.add(File {
        path: PathBuf::from("/etc/apt/preferences.d/ppa.pref"),
        content: "Package: *\n\
                  Pin: origin ppa.launchpad.net\n\
                  Pin-Priority: 1\n\
                  \n\
                  Package: *\n\
                  Pin: origin ppa.launchpadcontent.net\n\
                  Pin-Priority: 1\n"
            .to_string(),
        mode: mode_644(),
        ..Default::default()
    });

    // Mirror selection: arm hosts use ports.ubuntu.com (no archive.ubuntu.com
    // builds), everything else uses archive.ubuntu.com / security.ubuntu.com.
    let (archive_base, security_base) = if ctx.env.is_arm() {
        (
            "http://ports.ubuntu.com/ubuntu-ports",
            "http://ports.ubuntu.com/ubuntu-ports",
        )
    } else {
        (
            "http://archive.ubuntu.com/ubuntu",
            "http://security.ubuntu.com/ubuntu",
        )
    };

    // Render one (pin, list) pair per entry in ctx.config.apt_repos. Mirrors
    // the legacy role's `with_items: "{{ apt_repos }}"` loop over the same
    // template names.
    let mut repo_resources: Vec<ResourceId> = vec![sources_list, ppa_pin];
    let mut ppa_safronov_enabled = false;
    for repo in &ctx.config.apt_repos {
        let stem = repo.stem();
        let (pin_body, list_body) = render_repo_entry(*repo, codename, archive_base, security_base);
        let pin = ctx.plan.add(File {
            path: PathBuf::from(format!("/etc/apt/preferences.d/{stem}.pref")),
            content: pin_body,
            mode: mode_644(),
            ..Default::default()
        });
        let list = ctx.plan.add(File {
            path: PathBuf::from(format!("/etc/apt/sources.list.d/{stem}.list")),
            content: list_body,
            mode: mode_644(),
            ..Default::default()
        });
        repo_resources.push(pin);
        repo_resources.push(list);
        if *repo == AptRepo::PpaPvSafronovBackports {
            ppa_safronov_enabled = true;
        }
    }

    // Per the legacy role's `apt_key` task: fetch the PPA signing key into
    // its own keyring file under /etc/apt/trusted.gpg.d/. Only emitted when
    // the user opted into the PPA via apt_repos.
    if ppa_safronov_enabled {
        let key = ctx.plan.add(Command {
            name: "fetch ppa-pv-safronov-backports signing key".to_string(),
            argv: vec![
                "gpg".to_string(),
                "--no-default-keyring".to_string(),
                "--keyring".to_string(),
                "/etc/apt/trusted.gpg.d/ppa-pv-safronov-backports.gpg".to_string(),
                "--keyserver".to_string(),
                "keyserver.ubuntu.com".to_string(),
                "--recv-keys".to_string(),
                "FED902047AF1397755144CF6B47BBF2062DDDB70".to_string(),
            ],
            ..Default::default()
        });
        repo_resources.push(key);
    }

    // Bootstrap packages match roles/apt/tasks/packages.yml verbatim. Several
    // (python3-apt, python3-pycurl, software-properties-common, aptitude) are
    // only relevant when ansible itself drives apt; kept for now so the
    // framework can be applied to ansible-managed hosts without diverging.
    // apt-get update is run by the AptPackage batcher itself before any
    // install, so no explicit Command is needed in this bundle.
    let mut deps_for_pkgs: Vec<ResourceId> = confd_files.clone();
    deps_for_pkgs.extend(repo_resources.iter().copied());
    deps_for_pkgs.extend(cleanup_files.iter().copied());
    let bootstrap_ids: Vec<_> = [
        "software-properties-common",
        "python3-pycurl",
        "python3-apt",
        "apt-transport-https",
        "openssh-client",
        "aptitude",
    ]
    .iter()
    .map(|name| {
        ctx.plan.add(AptPackage {
            name: (*name).to_string(),
            deps: deps_for_pkgs.clone(),
            ..Default::default()
        })
    })
    .collect();

    let mut all_apt_resources = confd_files;
    all_apt_resources.extend(cleanup_files);
    all_apt_resources.extend(repo_resources);
    all_apt_resources.extend(bootstrap_ids);

    ctx.plan.add(Marker {
        name: "apt:ready".to_string(),
        deps: all_apt_resources,
        ..Default::default()
    })
}

// Render the (pin body, list body) pair for one apt_repos entry. Bodies are
// byte-identical to the corresponding j2 templates under
// roles/apt/templates/etc/apt/{preferences.d,sources.list.d}/.
fn render_repo_entry(
    repo: AptRepo,
    codename: &str,
    archive_base: &str,
    security_base: &str,
) -> (String, String) {
    match repo {
        AptRepo::Ubuntu => (
            format!("Package: *\nPin: release a={codename}\nPin-Priority: 990\n"),
            format!(
                "deb {archive_base}/ {codename} main restricted universe multiverse\n\
                 deb-src {archive_base}/ {codename} main restricted universe multiverse\n",
            ),
        ),
        AptRepo::UbuntuSecurity => (
            format!("Package: *\nPin: release a={codename}-security\nPin-Priority: 990\n"),
            format!(
                "deb {security_base}/ {codename}-security main restricted universe multiverse\n\
                 deb-src {security_base}/ {codename}-security main restricted universe multiverse\n",
            ),
        ),
        AptRepo::UbuntuUpdates => (
            format!("Package: *\nPin: release a={codename}-updates\nPin-Priority: 990\n"),
            format!(
                "deb {archive_base}/ {codename}-updates main restricted universe multiverse\n\
                 deb-src {archive_base}/ {codename}-updates main restricted universe multiverse\n",
            ),
        ),
        AptRepo::UbuntuBackports => (
            format!("Package: *\nPin: release a={codename}-backports\nPin-Priority: 990\n"),
            format!(
                "deb {archive_base}/ {codename}-backports main restricted universe multiverse\n\
                 deb-src {archive_base}/ {codename}-backports main restricted universe multiverse\n",
            ),
        ),
        // ubuntu-proposed uses archive.ubuntu.com (or ports.ubuntu.com on
        // arm), never security.ubuntu.com, per the legacy template.
        AptRepo::UbuntuProposed => (
            format!("Package: *\nPin: release a={codename}-proposed\nPin-Priority: 990\n"),
            format!(
                "deb {archive_base}/ {codename}-proposed main restricted universe multiverse\n\
                 deb-src {archive_base}/ {codename}-proposed main restricted universe multiverse\n",
            ),
        ),
        AptRepo::PpaPvSafronovBackports => (
            "Package: *\n\
             Pin: release o=LP-PPA-pv-safronov-backports\n\
             Pin-Priority: 990\n"
                .to_string(),
            format!(
                "deb https://ppa.launchpadcontent.net/pv-safronov/backports/ubuntu {codename} main\n\
                 deb-src https://ppa.launchpadcontent.net/pv-safronov/backports/ubuntu {codename} main\n",
            ),
        ),
    }
}
