mod commands;
mod state;
mod types;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::open_collection,
            commands::send_request,
            commands::list_environments,
            commands::save_request,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Wire");
}
