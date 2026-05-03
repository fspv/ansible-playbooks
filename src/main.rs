use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, ValueEnum};
use host_setup::bundles;
use host_setup::config::Config;
use host_setup::env::{Env, RunMode};
use host_setup::executor::Executor;
use host_setup::plan::Plan;
use host_setup::resource::Changed;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "host-setup", about = "Declarative host configuration")]
struct Args {
    /// Bundles to apply. Repeat to run several in one Plan
    /// (`--bundle apt --bundle users`). Defaults to all known bundles when
    /// omitted.
    #[arg(long = "bundle", value_enum)]
    bundles: Vec<BundleName>,

    /// Path to a YAML config file (per-host data: users, ...). Defaults to
    /// nothing — bundles that need config will see an empty Config.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Sense state and report what would change without modifying the
    /// system. Read-only commands still run; writes and apt-get install
    /// are skipped.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BundleName {
    ApcUps,
    Apparmor,
    Apt,
    CaCert,
    Chrony,
    Common,
    CommonDevserver,
    CommonTools,
    CommonTweaks,
    Devserver,
    Docker,
    Et,
    Flatpak,
    Gpg,
    Iptables,
    Libvirtd,
    Locales,
    Nix,
    NodeExporter,
    Nvidia,
    Nvim,
    Pip,
    SmartctlExporter,
    Snapd,
    Tuxedo,
    Tzdata,
    UbuntuDevserver,
    Users,
    Yubico,
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();
    let args = Args::parse();

    let run_mode = if args.dry_run {
        RunMode::DryRun
    } else {
        RunMode::Apply
    };
    let env = Arc::new(Env::detect().with_run_mode(run_mode));

    let cfg = match args.config.as_deref() {
        Some(path) => match Config::load(path) {
            Ok(cfg) => cfg,
            Err(e) => {
                error!(error = %e, "config load failed");
                return ExitCode::FAILURE;
            }
        },
        None => Config::default(),
    };

    // Default-no-flag is equivalent to running the legacy `common-devserver.yml`
    // playbook. To run something narrower or wider, the user passes one or
    // more `--bundle` flags explicitly. Each bundle's own `build` body
    // pulls in its transitive deps via `ctx.<other>()` calls.
    let bundles_to_run: Vec<BundleName> = if args.bundles.is_empty() {
        vec![BundleName::CommonDevserver]
    } else {
        args.bundles.clone()
    };

    let mut plan = Plan::new();
    {
        let mut ctx = bundles::Context::new(&mut plan, &env, &cfg);
        for bundle in &bundles_to_run {
            dispatch(&mut ctx, *bundle);
        }
    }

    if args.dry_run {
        warn!("DRY RUN — no changes will be applied");
    }
    info!(resources = plan.len(), bundles = ?bundles_to_run, dry_run = args.dry_run, "running plan");

    let yes_label = if args.dry_run {
        "would-change"
    } else {
        "changed"
    };

    match Executor::new(plan).run(env).await {
        Ok(report) => {
            for outcome in &report.outcomes {
                let status = match outcome.changed {
                    Changed::Yes => yes_label,
                    Changed::No => "unchanged",
                    Changed::Skipped => "skipped",
                };
                info!(resource = %outcome.id_hint, status);
            }
            let changed_count = report.count(Changed::Yes);
            if args.dry_run {
                info!(
                    would_change = changed_count,
                    unchanged = report.count(Changed::No),
                    skipped = report.count(Changed::Skipped),
                    "done (dry run)",
                );
            } else {
                info!(
                    changed = changed_count,
                    unchanged = report.count(Changed::No),
                    skipped = report.count(Changed::Skipped),
                    "done",
                );
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            error!(error = %e, "plan run failed");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(ctx: &mut bundles::Context<'_>, bundle: BundleName) {
    match bundle {
        BundleName::ApcUps => {
            ctx.apc_ups();
        }
        BundleName::Apparmor => {
            ctx.apparmor();
        }
        BundleName::Apt => {
            ctx.apt();
        }
        BundleName::CaCert => {
            ctx.ca_cert();
        }
        BundleName::Chrony => {
            ctx.chrony();
        }
        BundleName::Common => {
            ctx.common();
        }
        BundleName::CommonDevserver => {
            ctx.common_devserver();
        }
        BundleName::CommonTools => {
            ctx.common_tools();
        }
        BundleName::CommonTweaks => {
            ctx.common_tweaks();
        }
        BundleName::Devserver => {
            ctx.devserver();
        }
        BundleName::Docker => {
            ctx.docker();
        }
        BundleName::Et => {
            ctx.et();
        }
        BundleName::Flatpak => {
            ctx.flatpak();
        }
        BundleName::Gpg => {
            ctx.gpg();
        }
        BundleName::Iptables => {
            ctx.iptables();
        }
        BundleName::Libvirtd => {
            ctx.libvirtd();
        }
        BundleName::Locales => {
            ctx.locales();
        }
        BundleName::Nix => {
            ctx.nix();
        }
        BundleName::NodeExporter => {
            ctx.node_exporter();
        }
        BundleName::Nvidia => {
            ctx.nvidia();
        }
        BundleName::Nvim => {
            ctx.nvim();
        }
        BundleName::Pip => {
            ctx.pip();
        }
        BundleName::SmartctlExporter => {
            ctx.smartctl_exporter();
        }
        BundleName::Snapd => {
            ctx.snapd();
        }
        BundleName::Tuxedo => {
            ctx.tuxedo();
        }
        BundleName::Tzdata => {
            ctx.tzdata();
        }
        BundleName::UbuntuDevserver => {
            ctx.ubuntu_devserver();
        }
        BundleName::Users => {
            ctx.users();
        }
        BundleName::Yubico => {
            ctx.yubico();
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,host_setup=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
