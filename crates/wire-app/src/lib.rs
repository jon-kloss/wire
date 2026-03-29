mod commands;
mod state;
mod types;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::open_collection,
            commands::send_request,
            commands::send_raw_request,
            commands::list_environments,
            commands::list_history,
            commands::clear_history,
            commands::create_collection_cmd,
            commands::rename_collection_cmd,
            commands::scan_codebase,
            commands::get_environment,
            commands::save_environment,
            commands::read_request,
            commands::save_request,
            commands::evaluate_tests,
            commands::list_templates_cmd,
            commands::read_template,
            commands::save_template,
            commands::delete_template,
            commands::toggle_default_template,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Wire");
}
