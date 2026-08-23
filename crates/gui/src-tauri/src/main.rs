// mac-cleaner desktop GUI shell. Thin Tauri layer: every command delegates to
// the tested `macclean-gui-core`, which routes all deletion through the
// consent-gated executor in `macclean-core`. No deletion logic lives here.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{AppHandle, Emitter};

use macclean_core::loginitems::LoginItem;
use macclean_core::report::ScanReport;
use macclean_gui_core::{self as gui, CleanSummary, Expected, Filters, Permissions};

/// Event channel the frontend listens on for scan progress.
const SCAN_PROGRESS: &str = "scan://progress";

/// The one URL this app will ever open. Hardcoded on purpose: granting the
/// webview a general "open a URL" permission would let any future frontend bug
/// — or anything that reached the frontend — launch an arbitrary handler. One
/// constant, one destination, nothing to abuse.
const FULL_DISK_ACCESS_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

/// Scanning a real home walks ~165k files and is I/O-bound: ~8s warm, ~37s cold.
/// Run it on the blocking pool. A `#[tauri::command]` that is not `async` runs
/// inline on the webview's message loop, which freezes the window — no repaint,
/// no input — for the entire scan.
#[tauri::command]
async fn scan(app: AppHandle, filters: Filters) -> Result<ScanReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = gui::default_home().map_err(|e| e.to_string())?;
        let report = gui::scan_report_with_progress(&home, &filters, &mut |p| {
            // A dropped progress event is cosmetic; never fail a scan over one.
            let _ = app.emit(SCAN_PROGRESS, p);
        });
        Ok(report)
    })
    .await
    .map_err(|e| format!("scan task failed: {e}"))?
}

#[tauri::command]
async fn login_items() -> Result<Vec<LoginItem>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let home = gui::default_home().map_err(|e| e.to_string())?;
        Ok(gui::list_login_items(&home))
    })
    .await
    .map_err(|e| format!("login-items task failed: {e}"))?
}

/// Read-only probe of the TCC-gated roots, so the UI can warn that a scan may be
/// under-reporting instead of quietly showing a smaller number than the truth.
#[tauri::command]
async fn permissions() -> Result<Permissions, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let home = gui::default_home().map_err(|e| e.to_string())?;
        Ok(gui::probe_permissions(&home))
    })
    .await
    .map_err(|e| format!("permissions task failed: {e}"))?
}

/// Open the Full Disk Access pane in System Settings. Takes no arguments — see
/// `FULL_DISK_ACCESS_PANE`.
#[tauri::command]
async fn open_privacy_settings() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let status = std::process::Command::new("/usr/bin/open")
            .arg(FULL_DISK_ACCESS_PANE)
            .status()
            .map_err(|e| format!("couldn't open System Settings: {e}"))?;
        if status.success() {
            Ok(())
        } else {
            Err("System Settings didn't open".to_string())
        }
    })
    .await
    .map_err(|e| format!("open task failed: {e}"))?
}

/// Move the selected categories' junk to the Trash via the consent-gated
/// executor. `confirm_mass_delete` is the user's explicit modal confirmation,
/// and `expected` is the count/size that confirmation was shown against — the
/// plan is rebuilt here, so a materially larger one is refused rather than
/// executed. Trash-only (never permanent); routes entirely through macclean-core.
#[tauri::command]
async fn clean(
    filters: Filters,
    categories: Vec<String>,
    expected: Option<Expected>,
    confirm_mass_delete: bool,
) -> Result<CleanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        gui::clean(&filters, categories, expected, confirm_mass_delete)
    })
    .await
    .map_err(|e| format!("clean task failed: {e}"))?
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            scan,
            login_items,
            permissions,
            open_privacy_settings,
            clean
        ])
        .run(tauri::generate_context!())
        .expect("error while running mac-cleaner");
}
