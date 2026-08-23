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
use macclean_core::loginitems::{self, LoginItem};
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
        /// Only consider files at least this large (e.g. 100M, 1G, 500K, 4096).
        #[arg(long, value_parser = parse_size)]
        min_size: Option<u64>,
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
        /// Only consider files at least this large (e.g. 100M, 1G, 500K, 4096).
        #[arg(long, value_parser = parse_size)]
        min_size: Option<u64>,
        /// Path to the append-only audit log
        /// (default: ~/Library/Application Support/macclean/audit.jsonl).
        #[arg(long)]
        audit: Option<PathBuf>,
    },
    /// List login items (LaunchAgents) — read-only startup review.
    LoginItems {
        /// Emit as JSON instead of a table.
        #[arg(long)]
        json: bool,
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
            min_size,
            json,
        } => {
            let cfg = build_config(home, older_than_days, min_size);
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
            min_size,
            audit,
        } => {
            let cfg = build_config(home.clone(), older_than_days, min_size);
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
        Cmd::LoginItems { json } => {
            let items = loginitems::scan_dir(&loginitems::default_dir(&home));
            if json {
                println!("{}", loginitems::to_json_pretty(&items));
            } else {
                print_login_items(&items);
            }
            Ok(ExitCode::SUCCESS)
        }
    }
}

fn print_login_items(items: &[LoginItem]) {
    if items.is_empty() {
        println!("No login items found in ~/Library/LaunchAgents.");
        return;
    }
    println!("Login items (~/Library/LaunchAgents):");
    for it in items {
        let status = if it.disabled {
            "disabled"
        } else if it.run_at_load {
            "runs at login"
        } else {
            "on demand"
        };
        println!(
            "  {:<40} {:<14} {}",
            it.label,
            status,
            it.program.as_deref().unwrap_or("-")
        );
    }
    let active = items
        .iter()
        .filter(|i| i.run_at_load && !i.disabled)
        .count();
    println!("\n{active} item(s) run at login. Disable any you don't need to speed up startup.");
}

/// Build a scan config for `home`, applying the optional age and size filters.
fn build_config(home: PathBuf, older_than_days: Option<u64>, min_size: Option<u64>) -> ScanConfig {
    let mut cfg = ScanConfig::with_default_roots(home);
    if let Some(days) = older_than_days {
        cfg = cfg.older_than(Duration::from_secs(days.saturating_mul(86_400)));
    }
    if let Some(bytes) = min_size {
        cfg = cfg.min_size(bytes);
    }
    cfg
}

/// Parse a human size into bytes. Accepts a bare number (bytes) or a binary
/// suffix K/M/G/T (optionally followed by `B`/`iB`), case-insensitive:
/// `4096`, `500K`, `100M`, `2G`, `1TiB`.
fn parse_size(input: &str) -> Result<u64, String> {
    let lowered = input.trim().to_ascii_lowercase();
    let trimmed = lowered
        .strip_suffix("ib")
        .or_else(|| lowered.strip_suffix('b'))
        .unwrap_or(&lowered);
    let (number, multiplier) = match trimmed.chars().last() {
        Some('k') => (&trimmed[..trimmed.len() - 1], 1024u64),
        Some('m') => (&trimmed[..trimmed.len() - 1], 1024u64.pow(2)),
        Some('g') => (&trimmed[..trimmed.len() - 1], 1024u64.pow(3)),
        Some('t') => (&trimmed[..trimmed.len() - 1], 1024u64.pow(4)),
        _ => (trimmed, 1u64),
    };
    let value: u64 = number
        .trim()
        .parse()
        .map_err(|_| format!("invalid size: {input:?} (try e.g. 100M, 1G, 4096)"))?;
    value
        .checked_mul(multiplier)
        .ok_or_else(|| format!("size too large: {input:?}"))
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

    // Check BEFORE creating anything: creating first and refusing afterwards
    // would leave directories behind inside a location we then decline to
    // touch. `canonicalize` needs the path to exist, so test it lexically here
    // — `is_protected` fails closed on `..`, so an unresolved path is safe to
    // ask about. (Note we check the *requested* parent, not its nearest
    // existing ancestor: `~` is itself protected, but that means "never remove
    // the home directory", not "never create anything inside it".)
    if safety::denylist::is_protected(parent, home) {
        return Err(format!(
            "refused: audit log directory is protected: {}",
            parent.display()
        )
        .into());
    }

    // The lexical check above cannot see through symlinks, and `create_dir_all`
    // happily follows an intermediate one (mkdir(2) only refuses to follow the
    // *final* component). So resolve the deepest ancestor that actually exists,
    // re-append the tail we would create, and ask where those directories would
    // really land.
    //
    // Note we check the *reconstructed* path, not the existing ancestor itself:
    // `~` and `~/Library` are both protected, yet creating
    // `~/Library/Application Support/macclean` inside them is perfectly
    // legitimate. "Protected" means never destroy this, not never create here.
    let existing = parent
        .ancestors()
        .find(|a| a.exists())
        .ok_or("audit log path has no existing ancestor directory")?;
    let tail = parent.strip_prefix(existing).unwrap_or(Path::new(""));
    let would_land_at = std::fs::canonicalize(existing)?.join(tail);
    if safety::denylist::is_protected(&would_land_at, home) {
        return Err(format!(
            "refused: audit log directory would resolve into a protected location: {}",
            would_land_at.display()
        )
        .into());
    }

    std::fs::create_dir_all(parent)?;
    // Re-check the now-resolved parent: creation may have followed a symlink
    // into somewhere protected.
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

#[cfg(test)]
mod tests {
    use super::{parse_size, resolve_audit_path};

    #[test]
    fn parses_bare_bytes() {
        assert_eq!(parse_size("4096").unwrap(), 4096);
        assert_eq!(parse_size("  500 ").unwrap(), 500);
    }

    #[test]
    fn parses_binary_suffixes() {
        assert_eq!(parse_size("1K").unwrap(), 1024);
        assert_eq!(parse_size("2m").unwrap(), 2 * 1024 * 1024);
        assert_eq!(parse_size("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("5KiB").unwrap(), 5 * 1024);
        assert_eq!(parse_size("10mb").unwrap(), 10 * 1024 * 1024);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_size("abc").is_err());
        assert!(parse_size("").is_err());
        assert!(parse_size("1.5G").is_err());
    }

    #[test]
    fn audit_path_refuses_a_protected_directory_without_creating_it() {
        let dir = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(dir.path()).unwrap();

        // ~/Library/Keychains is protected, and so is anything we would create
        // beneath it. The refusal must happen before mkdir, so the tree stays
        // untouched.
        let target = home.join("Library/Keychains/deep/nested/audit.jsonl");
        let err = resolve_audit_path(Some(target), &home).unwrap_err();
        assert!(
            err.to_string().contains("protected"),
            "expected a protection refusal, got: {err}"
        );
        assert!(
            !home.join("Library/Keychains").exists(),
            "refused path must not be created on the way to refusing it"
        );
    }

    #[test]
    fn audit_path_accepts_the_default_location() {
        let dir = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(dir.path()).unwrap();
        let resolved = resolve_audit_path(None, &home).unwrap();
        assert!(resolved.ends_with("audit.jsonl"));
        assert!(resolved.starts_with(&home));
    }

    #[test]
    fn audit_path_refuses_a_symlink_into_a_protected_location() {
        let dir = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(home.join("Library/Keychains")).unwrap();
        std::fs::create_dir_all(home.join("safe")).unwrap();
        std::os::unix::fs::symlink(home.join("Library/Keychains"), home.join("safe/link")).unwrap();

        // Lexically this is just ~/safe/link/deep — nothing protected about it.
        // It resolves into Keychains, and create_dir_all would follow the link.
        let err =
            resolve_audit_path(Some(home.join("safe/link/deep/audit.jsonl")), &home).unwrap_err();
        assert!(
            err.to_string().contains("protected"),
            "expected a protection refusal, got: {err}"
        );
        assert!(
            !home.join("Library/Keychains/deep").exists(),
            "must not create a directory inside a protected location"
        );
    }
}
