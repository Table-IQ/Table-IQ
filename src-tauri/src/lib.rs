// TableIQ — Tauri entry. Slice 1: connect & browse.
pub mod bench;
pub mod connections;
pub mod drivers;
pub mod ipc;
pub mod query;

use ipc::connect::{
    connect, delete_connection, disconnect, get_table_window, list_connections, list_objects,
    open_table, save_connection, test_connection, ConnState,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
