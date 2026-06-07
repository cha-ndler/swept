//! `macclean` — safe, dry-run-first macOS junk cleaner.
//!
//! `scan` previews. `clean` previews too, unless `--execute` is passed. Even
//! with `--execute`, files go to the Trash unless `--permanent` is given, and a
//! mass delete needs `--yes`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};

use macclean_core::audit::AuditLog;
use macclean_core::executor::{execute, Consent, SystemSink};
use macclean_core::plan::{Plan, MASS_DELETE_BYTES, MASS_DELETE_COUNT};
use macclean_core::report::ScanReport;
use macclean_core::scanner::{scan, ScanConfig};
use safety::canonical_home;

#[derive(Parser)]
#[command(
    name = "macclean",
    version,
    about = "Safe, dry-run-first macOS junk cleaner"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Preview what would be cleaned. Never changes anything.
    Scan {
        /// Only consider files not modified in the last N days.
        #[arg(long)]
        older_than_days: Option<u64>,
        /// Emit the plan as JSON (for scripts / the GUI) instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Clean. Dry-run unless --execute is given.
    Clean {
        /// Actually carry out the actions (otherwise this is a preview).
        #[arg(long)]
        execute: bool,
        /// Permanently delete instead of moving to Trash (irreversible).
        #[arg(long)]
        permanent: bool,
        /// Confirm a mass delete (required past the safety threshold).
        #[arg(long = "yes")]
        confirm: bool,
        /// Only consider files not modified in the last N days.
        #[arg(long)]
        older_than_days: Option<u64>,
        /// Path to the append-only audit log
        /// (default: ~/Library/Application Support/macclean/audit.jsonl).
        #[arg(long)]
        audit: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let home = dirs::home_dir().ok_or("cannot determine home directory")?;
    let home = canonical_home(&home)?;

    match cli.cmd {
        Cmd::Scan {
            older_than_days,
            json,
        } => {
            let cfg = build_config(home, older_than_days);
            let plan = scan(&cfg);
            if json {
                println!("{}", ScanReport::from_plan(&plan).to_json_pretty());
            } else {
                print_plan(&plan);
                println!("\nThis was a preview. Run `macclean clean --execute` to act on it.");
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Clean {
            execute: do_exec,
            permanent,
            confirm,
            older_than_days,
            audit,
        } => {
            let cfg = build_config(home.clone(), older_than_days);
            let plan = scan(&cfg);
            print_plan(&plan);

            if !do_exec {
                println!("\nPreview only (no --execute). Nothing was changed.");
                return Ok(ExitCode::SUCCESS);
            }

            if plan.requires_confirmation() && !confirm {
                eprintln!(
                    "\nrefused: this would remove {} items / {} — pass --yes to confirm a mass delete.",
                    plan.count(),
                    human_bytes(plan.total_bytes())
                );
                return Ok(ExitCode::FAILURE);
            }

            let consent = Consent {
                execute: true,
                allow_permanent: permanent,
                confirmed_mass_delete: confirm,
            };
            let audit_path = resolve_audit_path(audit, &home)?;
            let mut log = AuditLog::open(&audit_path)?;
            let report = execute(&plan, consent, &home, &SystemSink, &mut log)?;
            println!(
                "\nDone: {} removed ({} freed), {} refused. Audit: {}",
                report.executed,
                human_bytes(report.bytes_executed),
                report.refused,
                audit_path.display()
            );
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Build a scan config for `home`, applying the optional age filter.
fn build_config(home: PathBuf, older_than_days: Option<u64>) -> ScanConfig {
    let cfg = ScanConfig::with_default_roots(home);
    match older_than_days {
        Some(days) => cfg.older_than(Duration::from_secs(days.saturating_mul(86_400))),
        None => cfg,
    }
}

/// Resolve the audit-log path to an absolute location, create its parent, and
/// refuse if that parent is on the protected denylist (the audit file is the one
/// write path that does not otherwise pass through `guard`).
fn resolve_audit_path(
    arg: Option<PathBuf>,
    home: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let requested =
        arg.unwrap_or_else(|| home.join("Library/Application Support/macclean/audit.jsonl"));
    let absolute = if requested.is_absolute() {
        requested
    } else {
        std::env::current_dir()?.join(requested)
    };
    let parent = absolute
        .parent()
        .ok_or("audit log path has no parent directory")?;
    std::fs::create_dir_all(parent)?;
    let canonical_parent = std::fs::canonicalize(parent)?;
    if safety::denylist::is_protected(&canonical_parent, home) {
        return Err(format!(
            "refused: audit log directory is protected: {}",
            canonical_parent.display()
        )
        .into());
    }
    let name = absolute
        .file_name()
        .ok_or("audit log path has no file name")?;
    Ok(canonical_parent.join(name))
}

fn print_plan(plan: &Plan) {
    if plan.actions.is_empty() {
        println!(
            "Nothing to clean. ({} candidates skipped by safety guard)",
            plan.skipped_protected
        );
        return;
    }
    let mut by_cat: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    for a in &plan.actions {
        let e = by_cat.entry(a.category.as_str()).or_insert((0, 0));
        e.0 += 1;
        e.1 += a.size_bytes;
    }
    println!("Cleanup plan:");
    for (cat, (count, bytes)) in &by_cat {
        println!("  {cat:<20} {count:>6} items  {:>10}", human_bytes(*bytes));
    }
    println!("  {:-<20} {:->6} ------  {:->10}", "", "", "");
    println!(
        "  {:<20} {:>6} items  {:>10}",
        "TOTAL",
        plan.count(),
        human_bytes(plan.total_bytes())
    );
    if plan.requires_confirmation() {
        println!(
            "\n  ! mass delete: exceeds {} items or {} — needs --yes to execute.",
            MASS_DELETE_COUNT,
            human_bytes(MASS_DELETE_BYTES)
        );
    }
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}
