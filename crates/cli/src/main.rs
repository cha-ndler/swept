//! `swept` — safe, dry-run-first macOS junk cleaner.
//!
//! `scan` previews. `clean` previews too, unless `--execute` is passed. Even
//! with `--execute`, files go to the Trash unless `--permanent` is given, and a
//! mass delete needs `--yes`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};

use safety::canonical_home;
use swept_core::audit::AuditLog;
use swept_core::executor::{execute, Consent, SystemSink};
use swept_core::loginitems::{self, LoginItem, StartClass};
use swept_core::plan::{Plan, MASS_DELETE_BYTES, MASS_DELETE_COUNT};
use swept_core::privacy;
use swept_core::report::ScanReport;
use swept_core::scanner::{scan, ScanConfig};

#[derive(Parser)]
#[command(
    name = "swept",
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
        /// (default: ~/Library/Application Support/swept/audit.jsonl).
        #[arg(long)]
        audit: Option<PathBuf>,
    },
    /// List login items (LaunchAgents) — read-only startup review.
    LoginItems {
        /// Emit as JSON instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Preview what browsers remember. Read-only; there is no `--execute`.
    ///
    /// Acting on any of this takes a per-path grant, which only the app can
    /// ask for. This subcommand exists so the search can be exercised against
    /// a real disk without one.
    Privacy,
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
                println!("\nThis was a preview. Run `swept clean --execute` to act on it.");
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
                granted: Vec::new(),
                granted_dirs: Vec::new(),
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
            let report = loginitems::scan(&loginitems::StartupConfig::new(home.clone()));
            let items = report.items;
            if json {
                println!("{}", loginitems::to_json_pretty(&items));
            } else {
                print_login_items(&items);
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Privacy => {
            print_privacy(&privacy::scan(&privacy::PrivacyConfig::new(home)));
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
        // The class, never the plist's `Disabled` key: that key is only the
        // initial value for a job launchd has not seen, and this app cannot
        // read the database that overrides it. Saying "disabled" here would be
        // a claim, not a reading.
        println!(
            "  {:<40} {:<32} {}",
            it.label,
            it.class.describe(),
            it.program.as_deref().unwrap_or("-")
        );
        if let Some(why) = &it.withheld {
            println!("  {:<40} {why}", "");
        }
    }
    let active = items
        .iter()
        .filter(|i| i.class == StartClass::StartsAtLogin)
        .count();
    println!("\n{active} item(s) start when you log in.");
    println!(
        "Most apps now register their login items with macOS directly; that list is in \
         System Settings › General › Login Items & Extensions."
    );
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
        arg.unwrap_or_else(|| home.join("Library/Application Support/swept/audit.jsonl"));
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
    // `~/Library/Application Support/swept` inside them is perfectly
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
    print!("{}", plan_summary(plan));
}

/// How many places the walk could not see into, phrased for a human.
fn gap_phrase(n: usize) -> String {
    format!("{n} place{}", if n == 1 { "" } else { "s" })
}

/// The plan as text. Pure, so what it claims can be tested.
///
/// The claim it must not make is the interesting one. `skipped_unreadable`
/// counts directories the walk could not open, and on a Mac without Full Disk
/// Access `~/.Trash` is one of them — a cleaner root, silently contributing
/// zero. Printing "Nothing to clean" over that is a conclusion the scan has
/// not earned, and printing a bare TOTAL over it is a completeness claim.
fn plan_summary(plan: &Plan) -> String {
    let mut out = String::new();
    let gap = plan.skipped_unreadable;

    // `count()`, not `actions.is_empty()`: one directory action is not one
    // item, and this is now a general function over any `Plan` rather than only
    // over what the scanner emits.
    if plan.count() == 0 {
        if gap > 0 {
            out.push_str("Nothing found in the locations that could be read.\n");
            out.push_str(&format!(
                "  ! the scan could not look inside {} — usually a location\n    \
                 behind Full Disk Access, so this is not an empty result.\n",
                gap_phrase(gap)
            ));
        } else {
            out.push_str("Nothing to clean.\n");
        }
        out.push_str(&format!(
            "  ({} candidates skipped by safety guard)\n",
            plan.skipped_protected
        ));
        return out;
    }

    let mut by_cat: BTreeMap<&str, (usize, u64)> = BTreeMap::new();
    for a in &plan.actions {
        let e = by_cat.entry(a.category.as_str()).or_insert((0, 0));
        e.0 += 1;
        e.1 += a.size_bytes;
    }
    out.push_str("Cleanup plan:\n");
    for (cat, (count, bytes)) in &by_cat {
        out.push_str(&format!(
            "  {cat:<20} {count:>6} items  {:>10}\n",
            human_bytes(*bytes)
        ));
    }
    out.push_str(&format!("  {:-<20} {:->6} ------  {:->10}\n", "", "", ""));
    // The number is the same either way; only the claim about it changes.
    out.push_str(&format!(
        "  {:<20} {:>6} items  {:>10}\n",
        if gap > 0 { "AT LEAST" } else { "TOTAL" },
        plan.count(),
        human_bytes(plan.total_bytes())
    ));
    if gap > 0 {
        out.push_str(&format!(
            "\n  ! a floor, not a total: the scan could not look inside {}.\n",
            gap_phrase(gap)
        ));
    }
    if plan.requires_confirmation() {
        out.push_str(&format!(
            "\n  ! mass delete: exceeds {} items or {} — needs --yes to execute.\n",
            MASS_DELETE_COUNT,
            human_bytes(MASS_DELETE_BYTES)
        ));
    }
    out
}

/// Read-only preview of what browsers remember.
///
/// Prints what is offered and, separately, what is shown and withheld — the
/// second list is the interesting one, because it is where this module refuses
/// to act rather than failing to look.
fn print_privacy(report: &privacy::PrivacyReport) {
    for browser in &report.browsers {
        let state = match &browser.access {
            // Safari has no profiles by construction, so a profile count would
            // read as a finding rather than a fact about how Safari is shaped.
            privacy::Access::Readable if browser.family == privacy::Family::Safari => {
                "readable".to_string()
            }
            privacy::Access::Readable if browser.profiles == 0 => {
                // The measured case: a vendor directory another installer
                // created, holding no profile the browser ever opened.
                "installed files, but no profile this has ever opened".to_string()
            }
            privacy::Access::Readable => format!("{} profile(s)", browser.profiles),
            privacy::Access::NotInstalled => "not installed".to_string(),
            privacy::Access::NeedsFullDiskAccess => {
                "needs Full Disk Access — grant it in System Settings › Privacy & Security"
                    .to_string()
            }
            privacy::Access::Unreadable(why) => format!("unreadable: {why}"),
        };
        let live = if browser.may_be_live {
            "  (looks like it is running)"
        } else {
            ""
        };
        println!("{:<22} {state}{live}", browser.name);
    }

    let (offered, withheld): (Vec<_>, Vec<_>) = report.rows.iter().partition(|r| r.offerable);

    println!("\nOffered ({}):", offered.len());
    for row in &offered {
        println!(
            "  {:>10}  {:<24} {}",
            human_bytes(row.size_bytes),
            row.label,
            row.path.display()
        );
    }

    println!("\nShown, not offered ({}):", withheld.len());
    for row in &withheld {
        println!(
            "  {:>10}  {:<24} {}",
            human_bytes(row.size_bytes),
            row.label,
            row.path.display()
        );
        if let Some(why) = &row.withheld {
            println!("              {why}");
        }
    }

    if !report.covered_elsewhere.is_empty() {
        println!("\nAlready covered by another category:");
        for covered in &report.covered_elsewhere {
            println!("  {} ({})", covered.path.display(), covered.category);
        }
    }
    if report.skipped_symlink > 0 {
        println!(
            "\n{} item(s) were symlinks and were not followed, so they are not \
             counted above.",
            report.skipped_symlink
        );
    }
    for caveat in &report.caveats {
        println!("\nnote: {caveat}");
    }
    println!(
        "\nTotal offered: {}{}",
        human_bytes(report.offerable_bytes()),
        if report.is_partial() {
            " (a floor — something could not be read)"
        } else {
            ""
        }
    );
    println!("\nThis was a preview. Nothing here can be removed from the command line.");
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

    // --- what the summary says about what it could not see -----------------

    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use swept_core::scanner::{scan, ScanConfig};

    fn fake_home() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::create_dir_all(home.join("Library/Caches/app")).unwrap();
        (dir, home)
    }

    fn put(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// Scan `home` with `locked` unopenable, restoring it afterwards so the
    /// tempdir can still be cleaned up.
    fn scan_with_locked(home: &Path, locked: &Path) -> swept_core::plan::Plan {
        let original = std::fs::metadata(locked).unwrap().permissions();
        let mut shut = original.clone();
        shut.set_mode(0o000);
        std::fs::set_permissions(locked, shut).unwrap();
        let plan = scan(&ScanConfig::with_default_roots(home.to_path_buf()));
        std::fs::set_permissions(locked, original).unwrap();
        plan
    }

    /// "Nothing to clean" is a conclusion. A scan that could not open a
    /// directory has not earned it — and on a Mac without Full Disk Access the
    /// directory it could not open is the Trash.
    #[test]
    fn an_empty_plan_with_a_gap_does_not_claim_nothing_to_clean() {
        let (_g, home) = fake_home();
        let locked = home.join("Library/Caches/locked");
        put(&locked.join("unseen.bin"), b"0123456789");

        let plan = scan_with_locked(&home, &locked);
        let out = super::plan_summary(&plan);

        assert!(plan.actions.is_empty());
        assert!(
            !out.contains("Nothing to clean"),
            "claimed a clean machine over a gap:\n{out}"
        );
        assert!(
            out.contains("could not"),
            "must say what it could not see:\n{out}"
        );
    }

    /// With a gap, the total is a floor and the wording has to carry that —
    /// the number itself is unchanged, so the number cannot say it.
    #[test]
    fn a_total_over_an_incomplete_scan_is_labelled_a_floor() {
        let (_g, home) = fake_home();
        put(&home.join("Library/Caches/app/seen.bin"), b"12345");
        let locked = home.join("Library/Caches/locked");
        put(&locked.join("unseen.bin"), b"0123456789");

        let plan = scan_with_locked(&home, &locked);
        let out = super::plan_summary(&plan);

        assert_eq!(plan.count(), 1);
        assert!(out.contains("AT LEAST"), "expected a floor label:\n{out}");
        assert!(out.contains("1 place"), "and the size of the gap:\n{out}");
    }

    /// The qualifier has to be able to be absent, or it stops meaning anything.
    #[test]
    fn a_complete_scan_says_total_plainly() {
        let (_g, home) = fake_home();
        put(&home.join("Library/Caches/app/seen.bin"), b"12345");

        let plan = scan(&ScanConfig::with_default_roots(home.clone()));
        let out = super::plan_summary(&plan);

        assert!(out.contains("TOTAL"), "{out}");
        assert!(!out.contains("AT LEAST"), "{out}");
        assert!(!out.contains("could not"), "{out}");
    }
}
