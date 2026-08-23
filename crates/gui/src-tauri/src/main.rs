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
        .setup(|app| {
            // Never fatal. The tray is a convenience; a failure to create it
            // must not stop the window from opening, which is what `?` here
            // would do. The label simply stays absent.
            if let Err(e) = build_tray(app.handle()) {
                eprintln!("mac-cleaner: menu-bar extra unavailable: {e}");
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // macOS convention: closing a window does not quit the app. Without
            // this the menu-bar extra would vanish the moment the window closed,
            // which is the opposite of what a menu-bar extra is for. Quit lives
            // in the tray menu and in Cmd-Q.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            scan,
            login_items,
            permissions,
            open_privacy_settings,
            set_tray_label,
            clean
        ])
        .run(tauri::generate_context!())
        .expect("error while running mac-cleaner");
}

// ---------------------------------------------------------------------------
// Menu-bar extra
//
// Deliberately has NO cleanup action. The roadmap originally called for a
// "quick-clean" item, but disposing of files straight from a menu means no
// preview and no confirmation, which the safety contract forbids outright
// (item 1: dry-run is the default; item 5: no unconfirmed mass delete). The
// tray surfaces the figure and opens the window; every disposal still goes
// through the same review-and-confirm path it always did.
// ---------------------------------------------------------------------------

const TRAY_ID: &str = "main";
const MENU_OPEN: &str = "open";
const MENU_QUIT: &str = "quit";
/// The menu-bar strip is narrow and shared with every other extra. A label
/// longer than this is not something the user wants sitting there.
const MAX_TRAY_LABEL: usize = 16;
/// Shown before the first scan finishes. There is no figure yet, and an em dash
/// says that honestly — unlike a zero, which would claim there is nothing to
/// reclaim.
const TRAY_PLACEHOLDER: &str = "—";

fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::TrayIconBuilder;

    let open = MenuItemBuilder::with_id(MENU_OPEN, "Open mac-cleaner").build(app)?;
    let quit = MenuItemBuilder::with_id(MENU_QUIT, "Quit mac-cleaner").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&open)
        .separator()
        .item(&quit)
        .build()?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("mac-cleaner")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_OPEN => show_main_window(app),
            MENU_QUIT => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    match app.default_window_icon() {
        Some(icon) => tray = tray.icon(icon.clone()),
        // Not fatal — the title alone keeps the item visible — but worth saying,
        // because an icon-less menu-bar extra is a packaging problem.
        None => eprintln!("mac-cleaner: no bundled icon for the menu-bar extra"),
    }

    // Never leave both the icon and the title unset: a status item with neither
    // is zero-width, so build() succeeds and nothing appears. This placeholder
    // is replaced by the real figure as soon as a scan completes.
    tray = tray.title(TRAY_PLACEHOLDER);

    tray.build(app)?;
    Ok(())
}

fn show_main_window(app: &tauri::AppHandle) {
    use tauri::Manager;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Put the latest reclaimable figure in the menu bar.
///
/// The frontend sends the string it is already displaying rather than a number
/// for us to re-format. Two formatters would eventually disagree, and a menu bar
/// that contradicts the window is exactly the kind of small dishonesty this app
/// is trying not to have. `None` clears it — after a failed scan there is no
/// honest figure to show.
#[tauri::command]
fn set_tray_label(app: AppHandle, label: Option<String>) -> Result<(), String> {
    let tray = app
        .tray_by_id(TRAY_ID)
        .ok_or_else(|| "no tray icon".to_string())?;
    let label = label
        .map(|l| l.trim().chars().take(MAX_TRAY_LABEL).collect::<String>())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| TRAY_PLACEHOLDER.to_string());
    tray.set_title(Some(&label)).map_err(|e| e.to_string())
}
