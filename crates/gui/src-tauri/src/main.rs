// mac-cleaner desktop GUI shell. Thin Tauri layer: every command delegates to
// the tested `macclean-gui-core`, which routes all deletion through the
// consent-gated executor in `macclean-core`. No deletion logic lives here.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use macclean_core::loginitems::LoginItem;
use macclean_core::report::ScanReport;
use macclean_gui_core::{self as gui, Filters};

#[tauri::command]
fn scan(filters: Filters) -> Result<ScanReport, String> {
    let home = gui::default_home().map_err(|e| e.to_string())?;
    Ok(gui::scan_report(&home, &filters))
}

#[tauri::command]
fn login_items() -> Result<Vec<LoginItem>, String> {
    let home = gui::default_home().map_err(|e| e.to_string())?;
    Ok(gui::list_login_items(&home))
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![scan, login_items])
        .run(tauri::generate_context!())
        .expect("error while running mac-cleaner");
}
