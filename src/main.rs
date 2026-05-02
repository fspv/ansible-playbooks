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
    #[arg(long, value_enum, default_value_t = BundleName::Apt)]
    bundle: BundleName,

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

#[derive(Debug, Clone, Copy, ValueEnum)]
enum BundleName {
    Apt,
    Users,
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

    let mut plan = Plan::new();
    match args.bundle {
        BundleName::Apt => bundles::apt::apply(&mut plan, &env),
        BundleName::Users => bundles::users::apply(&mut plan, &env, &cfg.users),
    }

    if args.dry_run {
        warn!("DRY RUN — no changes will be applied");
    }
    info!(resources = plan.len(), bundle = ?args.bundle, dry_run = args.dry_run, "running plan");

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

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,host_setup=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
