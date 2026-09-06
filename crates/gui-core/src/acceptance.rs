//! First-run acceptance of the Terms of Use.
//!
//! Swept is distributed outside the Mac App Store, so no platform agreement
//! sits between it and the person running it — the terms in `TERMS.md` and the
//! record written here are the whole contractual surface. `docs/LEGAL.md`
//! explains why this layer earns its keep: a disclaimer nobody agreed to is
//! worth considerably less than one they did, and the difference between the
//! two is a record like this one.
//!
//! **This module is additive and sits outside the deletion path.** It never
//! removes anything, and `executor::execute` is unaware of it — gating the one
//! chokepoint on a config file would add a new refusal mode to the most
//! safety-critical code in the project for no legal gain the assent record
//! does not already provide.
//!
//! Two properties are load-bearing:
//!
//! - **The terms are compiled into the binary** (`include_str!`), so the text
//!   the app shows is necessarily the text this build was made from — it cannot
//!   drift, and it needs no network and no URL-opening capability to display.
//!   The shell deliberately grants only two hardcoded URLs, and this stays
//!   compatible with that.
//! - **Everything fails closed.** A missing, unreadable, malformed or stale
//!   record all mean *not accepted*. The only way to be accepted is a
//!   well-formed record naming both the version and the exact text of the terms
//!   this build carries.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use swept_core::audit::now_ms;

/// The terms this build presents, compiled in from the repository root.
const TERMS_MD: &str = include_str!("../../../TERMS.md");

/// Version of `TERMS.md` this build ships.
///
/// Kept in sync with the document by a gate in `scripts/verify.sh` — the same
/// treatment the workspace version gets, and for the same reason: a constant
/// that can silently disagree with the file it describes is worse than no
/// constant at all.
pub const TERMS_VERSION: &str = "1.0";

/// Name of the record, written beside `audit.jsonl` so one folder holds the
/// whole history of what this app was told to do and what it was permitted to
/// do.
pub const ACCEPTANCE_FILE: &str = "acceptance.json";

/// Where Swept keeps its own data, relative to the home directory.
const DATA_DIR: &str = "Library/Application Support/swept";

/// One acceptance, as recorded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Acceptance {
    /// The `TERMS_VERSION` that was presented.
    pub terms_version: String,
    /// Digest of the exact text presented — see [`terms_digest`].
    pub terms_digest: String,
    /// The build that presented it.
    pub app_version: String,
    /// When it was accepted.
    pub epoch_ms: u64,
}

/// The on-disk shape: a history, not a flag.
///
/// Keeping every acceptance means a later revision of the terms leaves the
/// earlier agreement legible rather than overwriting it, which is the whole
/// point of having a record.
#[derive(Debug, Default, Serialize, Deserialize)]
struct AcceptanceFile {
    acceptances: Vec<Acceptance>,
}

/// What the frontend needs in order to decide whether to present the terms.
#[derive(Debug, Clone, Serialize)]
pub struct AcceptanceStatus {
    /// True only when the terms *this build carries* have been accepted.
    pub accepted: bool,
    /// The version this build would present.
    pub terms_version: &'static str,
    /// The digest this build would present.
    pub terms_digest: String,
    /// The most recent version the user accepted, if any. Distinguishes a
    /// first launch from a revision, so the UI can say "these terms have
    /// changed" rather than greeting a returning user as a stranger.
    pub accepted_version: Option<String>,
}

/// The full text of the terms this build presents.
pub fn terms_text() -> &'static str {
    TERMS_MD
}

/// A 64-bit FNV-1a digest of the terms text, in hex.
///
/// This is a **change detector**, not a security primitive: it answers "is this
/// the same document the user was shown?" and nothing else. Nothing trusts it
/// against an adversary who can already rewrite files in the home directory —
/// such an adversary does not need to forge an acceptance record, they can
/// remove the files directly. FNV is used so this needs no hash crate.
pub fn terms_digest() -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in TERMS_MD.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// The directory Swept keeps its record in.
fn data_dir(home: &Path) -> PathBuf {
    home.join(DATA_DIR)
}

/// Absolute path of the acceptance record.
pub fn record_path(home: &Path) -> PathBuf {
    data_dir(home).join(ACCEPTANCE_FILE)
}

/// Read the record and report whether *these* terms have been accepted.
///
/// Never creates anything and never fails: every error path is "not accepted".
pub fn status(home: &Path) -> AcceptanceStatus {
    let digest = terms_digest();
    let file = read_record(home).unwrap_or_default();

    // The last one recorded is what the user most recently agreed to.
    let accepted_version = file.acceptances.last().map(|a| a.terms_version.clone());
    // Accepted means *this* text, under *this* version. Same version with a
    // different digest is an edited document, and gets asked again.
    let accepted = file
        .acceptances
        .iter()
        .any(|a| a.terms_version == TERMS_VERSION && a.terms_digest == digest);

    AcceptanceStatus {
        accepted,
        terms_version: TERMS_VERSION,
        terms_digest: digest,
        accepted_version,
    }
}

/// Record that the terms this build carries were accepted.
///
/// Errors are returned rather than swallowed, and the caller must treat a
/// failure as a refusal to proceed — the same rule the audit log follows. If
/// consent cannot be recorded, we have no evidence it was given, and acting
/// anyway would be exactly the situation this module exists to prevent.
pub fn accept(home: &Path, app_version: &str) -> io::Result<Acceptance> {
    let dir = data_dir(home);

    // Fail closed if the record would land somewhere protected — the same
    // lexical-then-resolved check `resolve_audit_path` makes in the CLI, for
    // the same reason: a symlink at `~/Library/Application Support/swept` must
    // not turn a config write into a write inside `/System`.
    if safety::denylist::is_protected(&dir, home) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("refused: data directory is protected: {}", dir.display()),
        ));
    }
    if let Some(existing) = dir.ancestors().find(|a| a.exists()) {
        let tail = dir.strip_prefix(existing).unwrap_or(Path::new(""));
        let would_land_at = fs::canonicalize(existing)?.join(tail);
        if safety::denylist::is_protected(&would_land_at, home) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "refused: data directory would resolve into a protected location: {}",
                    would_land_at.display()
                ),
            ));
        }
    }
    fs::create_dir_all(&dir)?;

    // Re-check after creating, and use the *resolved* directory from here on.
    // `create_dir_all` follows intermediate symlinks — `mkdir(2)` only refuses
    // to follow the final component — so the check above can be defeated by a
    // link swapped into place after it ran. The CLI's `resolve_audit_path` does
    // the same thing at the same point, and for the same reason.
    let dir = fs::canonicalize(&dir)?;
    if safety::denylist::is_protected(&dir, home) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!(
                "refused: data directory resolved into a protected location: {}",
                dir.display()
            ),
        ));
    }
    let path = dir.join(ACCEPTANCE_FILE);

    let entry = Acceptance {
        terms_version: TERMS_VERSION.to_string(),
        terms_digest: terms_digest(),
        app_version: app_version.to_string(),
        epoch_ms: now_ms(),
    };

    // An unparseable record is still evidence that someone accepted something.
    // Overwriting it silently would lose the only copy, so it is set aside
    // under a name that says what it is. `status` treats it as not-accepted
    // either way, so this cannot let a stale acceptance through.
    let mut file = match fs::read_to_string(&path) {
        Err(_) => AcceptanceFile::default(),
        Ok(raw) => match serde_json::from_str::<AcceptanceFile>(&raw) {
            Ok(existing) => existing,
            Err(_) => {
                let aside = dir.join(format!("{ACCEPTANCE_FILE}.unreadable-{}", now_ms()));
                fs::rename(&path, &aside)?;
                AcceptanceFile::default()
            }
        },
    };
    file.acceptances.push(entry.clone());

    // Staged under a random name in the same directory, then renamed over the
    // record. Two properties, both load-bearing:
    //
    //   * **Random, not predictable.** A fixed staging name is a planted-symlink
    //     primitive: opening it for writing follows the link and truncates
    //     whatever is on the other end, so `~/Documents/thesis.txt` could be
    //     replaced by this JSON. `NamedTempFile` creates with `O_EXCL`, which
    //     refuses to follow a link at all.
    //   * **Rename, not write.** `rename(2)` replaces the record rather than
    //     writing through it, so a crash mid-write leaves the previous history
    //     intact and a symlink at the record path is replaced rather than
    //     followed.
    let json = serde_json::to_string_pretty(&file).map_err(io::Error::other)?;
    let mut staged = tempfile::Builder::new()
        .prefix(".acceptance-")
        .suffix(".staged")
        .tempfile_in(&dir)?;
    staged.write_all(json.as_bytes())?;
    staged.as_file().sync_all()?;
    staged.persist(&path).map_err(|e| e.error)?;

    Ok(entry)
}

/// Read and parse the record. `None` for anything that is not a well-formed
/// record — absent, unreadable, or malformed all mean the same thing here.
fn read_record(home: &Path) -> Option<AcceptanceFile> {
    let raw = fs::read_to_string(record_path(home)).ok()?;
    serde_json::from_str(&raw).ok()
}
