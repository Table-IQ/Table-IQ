// TableIQ — Tauri entry.
pub mod ai;
pub mod bench;
pub mod connections;
pub mod drivers;
pub mod ipc;
pub mod query;

use ipc::connect::{
    ai_egress_preview, ai_generate_sql, apply_edit, connect, delete_connection, disconnect,
    export_result, export_table, get_ai_config, get_result_window, get_table_window,
    explain_query, list_connections, list_objects, open_table, primary_key, run_query, run_script,
    save_ai_config, save_connection, test_connection, ConnState,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(ConnState::new())
        .invoke_handler(tauri::generate_handler![
            list_connections,
            save_connection,
            delete_connection,
            test_connection,
            connect,
            disconnect,
            list_objects,
            open_table,
            get_table_window,
            run_query,
            get_result_window,
            primary_key,
            apply_edit,
            export_result,
            export_table,
            run_script,
            get_ai_config,
            save_ai_config,
            ai_egress_preview,
            ai_generate_sql,
            explain_query,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
