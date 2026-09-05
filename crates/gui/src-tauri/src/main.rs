// Swept desktop GUI shell. Thin Tauri layer: every command delegates to
// the tested `swept-gui-core`, which routes all deletion through the
// consent-gated executor in `swept-core`. No deletion logic lives here.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{AppHandle, Emitter};

use swept_core::loginitems::LoginItem;
use swept_core::report::ScanReport;
use swept_gui_core::smartscan::{SmartScanReportDto, SmartScanRequest, SmartScanRunReport};
use swept_gui_core::{
    self as gui, Acknowledged, CleanSummary, Expected, Filters, InstalledAppDto, LargeOldReportDto,
    Permissions, PrivacyReportDto, SpaceLensReportDto, StartupReportDto, StartupSummary,
    UninstallReportDto, UninstallTarget,
};

/// Event channel the frontend listens on for scan progress.
const SCAN_PROGRESS: &str = "scan://progress";

/// The one URL this app will ever open. Hardcoded on purpose: granting the
/// webview a general "open a URL" permission would let any future frontend bug
/// — or anything that reached the frontend — launch an arbitrary handler. One
/// constant, one destination, nothing to abuse.
const FULL_DISK_ACCESS_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles";

/// The second, and last, URL this app will ever open.
///
/// Verified on a reference machine rather than guessed: the identifier is the
/// `CFBundleIdentifier` of `/System/Library/ExtensionKit/Extensions/
/// LoginItems.appex`. It matters because most login items on a current Mac live
/// in a store this app can neither read nor change, and sending someone to the
/// wrong pane would be worse than not offering the route at all.
const LOGIN_ITEMS_PANE: &str = "x-apple.systempreferences:com.apple.LoginItems-Settings.extension";

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
/// executed. Trash-only (never permanent); routes entirely through swept-core.
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

/// Read-only: the largest (optionally oldest) files across the *discovery*
/// scope — `~/Documents`, `~/Downloads` and friends, far wider than anything
/// the app may clean unattended.
///
/// Returning a row here authorizes nothing. Acting on one takes `dispose_paths`
/// below, which re-guards every path individually.
#[tauri::command]
async fn large_and_old(
    min_size_bytes: u64,
    older_than_days: Option<u64>,
) -> Result<LargeOldReportDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = gui::default_home().map_err(|e| e.to_string())?;
        Ok(gui::large_and_old(&home, min_size_bytes, older_than_days))
    })
    .await
    .map_err(|e| format!("large-and-old task failed: {e}"))?
}

/// Read-only: every source Smart Scan can act on, in one call.
///
/// Four scans, so it is the slowest command here — on the blocking pool like
/// the rest. It authorizes nothing: the report it returns is a picture, and
/// acting on any part of it still goes through that module's own verb.
///
/// `filters` are the *cleaner* filters. Large & Old's threshold is deliberately
/// not exposed and is pinned to `DEFAULT_MIN_SIZE` — and the dispatcher enforces
/// it as a floor on what it will act on, which is what makes that pinning mean
/// anything. (It does not bound `dispose_paths` itself, whose ceiling is the
/// discovery scope; an earlier version of this comment claimed it did.)
#[tauri::command]
async fn smart_scan(filters: Filters) -> Result<SmartScanReportDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = gui::default_home().map_err(|e| e.to_string())?;
        Ok(gui::smartscan::smart_scan_in(
            &gui::smartscan::SmartScanConfig::new(home).with_filters(filters),
        ))
    })
    .await
    .map_err(|e| format!("smart-scan task failed: {e}"))?
}

/// Act on a confirmed Smart Scan.
///
/// Adds no disposal capability of its own: each source still goes through that
/// module's own verb, which re-runs its own scan inside the call and enforces
/// its own ceiling. Sequential and fail-fast — **no step begins after a step
/// refused** — and the ledger it returns distinguishes "we did not try" from
/// "we tried and there was nothing".
#[tauri::command]
async fn dispatch_smart_scan(request: SmartScanRequest) -> Result<SmartScanRunReport, String> {
    tauri::async_runtime::spawn_blocking(move || gui::smartscan::dispatch_smart_scan(request))
        .await
        .map_err(|e| format!("smart-scan dispatch task failed: {e}"))?
}

/// Read-only: the size of everything in the discovery scope, as a tree.
///
/// There is deliberately no companion command that takes a node back. Space
/// Lens is a picture of the disk — acting on something seen in it means finding
/// it in a module that can, and consenting there.
#[tauri::command]
async fn space_lens() -> Result<SpaceLensReportDto, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let home = gui::default_home().map_err(|e| e.to_string())?;
        Ok(gui::space_lens(&home))
    })
    .await
    .map_err(|e| format!("space-lens task failed: {e}"))?
}

/// Move individually-chosen paths to the Trash.
///
/// The only command that can act outside `allowlist::default_roots`, and it is
/// the strictest one: `gui-core` re-guards each path, requires each to already
/// be its own canonical spelling, confines them to the discovery scope, and
/// refuses the *whole* request if any item no longer matches what was listed.
/// Trash-only, never permanent.
#[tauri::command]
async fn dispose_paths(
    paths: Vec<String>,
    expected: Option<Expected>,
    confirm_mass_delete: bool,
) -> Result<CleanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        gui::dispose_selected(paths, expected, confirm_mass_delete)
    })
    .await
    .map_err(|e| format!("dispose task failed: {e}"))?
}

/// Read-only: the applications a user may pick from. Top-level bundles only.
#[tauri::command]
async fn installed_apps() -> Result<Vec<InstalledAppDto>, String> {
    tauri::async_runtime::spawn_blocking(gui::installed_apps)
        .await
        .map_err(|e| format!("installed-apps task failed: {e}"))?
}

/// Read-only: what an application left behind. The frontend names an id and,
/// optionally, a display name — nothing else. It cannot set the home or the
/// inventory roots, because a frontend that could would be able to make an
/// installed app look uninstalled.
#[tauri::command]
async fn uninstall_leftovers(target: UninstallTarget) -> Result<UninstallReportDto, String> {
    tauri::async_runtime::spawn_blocking(move || gui::uninstall_leftovers(&target))
        .await
        .map_err(|e| format!("uninstall-leftovers task failed: {e}"))?
}

/// Move individually-chosen leftover rows to the Trash.
///
/// The strictest command in the app: `gui-core` re-runs discovery inside the
/// call and accepts a path only if it is byte-equal to an `offerable` row of
/// that fresh scan, re-guards each one (`guard_dir` for a tree), and refuses
/// the whole request if any item is not. Trash-only; a directory action cannot
/// express anything else.
#[tauri::command]
async fn dispose_leftovers(
    target: UninstallTarget,
    paths: Vec<String>,
    expected: Option<Expected>,
    confirm_mass_delete: bool,
) -> Result<CleanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        gui::dispose_leftovers(&target, paths, expected, confirm_mass_delete)
    })
    .await
    .map_err(|e| format!("dispose-leftovers task failed: {e}"))?
}

/// Read-only: what browsers remember. Takes nothing from the frontend, because
/// there is nothing it could usefully say — the browser table and the
/// recognised names are the module's own, and a frontend that could add to
/// either would be able to name a file this tool has never vetted.
#[tauri::command]
async fn privacy_report() -> Result<PrivacyReportDto, String> {
    tauri::async_runtime::spawn_blocking(gui::privacy_report)
        .await
        .map_err(|e| format!("privacy task failed: {e}"))?
}

/// Move individually-chosen privacy rows to the Trash.
///
/// Like `dispose_leftovers`, the ceiling is the `offerable` rows of a scan run
/// inside the call. On top of that, `acknowledged` is a second consent axis:
/// cookies, history and sessions each need their own explicit word, and it
/// defaults to granting none — so a frontend that loses its checkbox state
/// refuses rather than proceeds.
#[tauri::command]
async fn dispose_privacy(
    paths: Vec<String>,
    acknowledged: Acknowledged,
    expected: Option<Expected>,
    confirm_mass_delete: bool,
) -> Result<CleanSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        gui::dispose_privacy(paths, acknowledged, expected, confirm_mass_delete)
    })
    .await
    .map_err(|e| format!("dispose-privacy task failed: {e}"))?
}

/// Open the pane that holds the login items this app cannot see.
#[tauri::command]
async fn open_login_items_settings() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let status = std::process::Command::new("/usr/bin/open")
            .arg(LOGIN_ITEMS_PANE)
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

/// Read-only: what runs when you log in, plus what this app has set aside and
/// what it can see but never change.
#[tauri::command]
async fn startup_report() -> Result<StartupReportDto, String> {
    tauri::async_runtime::spawn_blocking(gui::startup_report)
        .await
        .map_err(|e| format!("startup task failed: {e}"))?
}

/// Take chosen items out of what starts at login — reversibly.
///
/// Nothing is removed: each plist is hard-linked into a folder beside it, the
/// inode is checked, and only then is the original name removed. The ceiling is
/// the rows of a scan run inside the call, so a system agent or a row the scan
/// withheld cannot be acted on by naming it.
#[tauri::command]
async fn move_aside(
    paths: Vec<String>,
    expected: Option<Expected>,
) -> Result<StartupSummary, String> {
    tauri::async_runtime::spawn_blocking(move || gui::move_aside(paths, expected))
        .await
        .map_err(|e| format!("move-aside task failed: {e}"))?
}

/// Put them back. The mirror of `move_aside`, with its own ceiling.
#[tauri::command]
async fn put_back(
    paths: Vec<String>,
    expected: Option<Expected>,
) -> Result<StartupSummary, String> {
    tauri::async_runtime::spawn_blocking(move || gui::put_back(paths, expected))
        .await
        .map_err(|e| format!("put-back task failed: {e}"))?
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // Never fatal. The tray is a convenience; a failure to create it
            // must not stop the window from opening, which is what `?` here
            // would do. The label simply stays absent.
            if let Err(e) = build_tray(app.handle()) {
                eprintln!("Swept: menu-bar extra unavailable: {e}");
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
            clean,
            large_and_old,
            dispose_paths,
            installed_apps,
            uninstall_leftovers,
            dispose_leftovers,
            privacy_report,
            dispose_privacy,
            startup_report,
            open_login_items_settings,
            move_aside,
            put_back,
            space_lens,
            smart_scan,
            dispatch_smart_scan
        ])
        .run(tauri::generate_context!())
        .expect("error while running Swept");
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
/// Generated by `icons/render-tray-template.mjs` from the same layered-stack
/// glyph the sidebar uses for Cleanup, so the menu bar and the app agree on what
/// this app's symbol is.
const TRAY_TEMPLATE_ICON: &[u8] = include_bytes!("../icons/tray-template.png");

fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::TrayIconBuilder;

    let open = MenuItemBuilder::with_id(MENU_OPEN, "Open Swept").build(app)?;
    let quit = MenuItemBuilder::with_id(MENU_QUIT, "Quit Swept").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&open)
        .separator()
        .item(&quit)
        .build()?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("Swept")
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

    // A menu-bar extra takes a *template* image: a black shape plus alpha that
    // the system recolours for light and dark menu bars and inverts when the
    // menu is open. The full-colour app icon is not one — at 22pt it reads as a
    // featureless square, which is exactly what the first build of this shipped.
    match tauri::image::Image::from_bytes(TRAY_TEMPLATE_ICON) {
        Ok(icon) => tray = tray.icon(icon).icon_as_template(true),
        // Not fatal — the title alone keeps the item visible — but worth saying,
        // because an icon-less menu-bar extra is a packaging problem.
        Err(e) => eprintln!("Swept: menu-bar icon failed to decode: {e}"),
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
