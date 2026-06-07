//! Read-only inspector for user LaunchAgents (login items).
//!
//! Reports what runs at login so the user can decide what to disable for faster
//! startup. This module **never modifies anything** — it only reads and parses
//! `.plist` files. Disabling an item is a separate, consent-gated action that is
//! intentionally not implemented here yet.

use std::path::{Path, PathBuf};

use serde::Serialize;

/// A single login item parsed from a LaunchAgent plist.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LoginItem {
    /// The launchd `Label` (falls back to the file stem if absent).
    pub label: String,
    /// The program path (`Program`, or the first of `ProgramArguments`).
    pub program: Option<String>,
    /// `RunAtLoad` — whether launchd starts it at login.
    pub run_at_load: bool,
    /// `Disabled` — whether it is marked disabled.
    pub disabled: bool,
    /// Absolute path of the source plist.
    pub source: String,
}

/// The default per-user LaunchAgents directory for `home`.
pub fn default_dir(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents")
}

/// Serialize a list of login items to pretty JSON.
pub fn to_json_pretty(items: &[LoginItem]) -> String {
    serde_json::to_string_pretty(items).unwrap_or_else(|_| "[]".to_string())
}

/// Parse every `.plist` in `dir` into a [`LoginItem`]. Read-only.
///
/// A missing directory yields an empty list; an individual plist that fails to
/// parse is skipped rather than aborting the scan. Results are sorted by label
/// for stable output.
pub fn scan_dir(dir: &Path) -> Vec<LoginItem> {
    let mut items = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return items;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("plist") {
            continue;
        }
        if let Some(item) = parse_login_item(&path) {
            items.push(item);
        }
    }
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items
}

fn parse_login_item(path: &Path) -> Option<LoginItem> {
    let value = plist::Value::from_file(path).ok()?;
    let dict = value.as_dictionary()?;

    let label = dict
        .get("Label")
        .and_then(|v| v.as_string())
        .map(str::to_string)
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

    let program = dict
        .get("Program")
        .and_then(|v| v.as_string())
        .map(str::to_string)
        .or_else(|| {
            dict.get("ProgramArguments")
                .and_then(|v| v.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_string())
                .map(str::to_string)
        });

    let run_at_load = dict
        .get("RunAtLoad")
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);
    let disabled = dict
        .get("Disabled")
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);

    Some(LoginItem {
        label,
        program,
        run_at_load,
        disabled,
        source: path.display().to_string(),
    })
}
