// TableIQ — Tauri entry.
pub mod ai;
pub mod bench;
pub mod connections;
pub mod drivers;
pub mod ipc;
pub mod library;
pub mod query;
pub mod ssh;

use ipc::connect::{
    ai_egress_preview, ai_generate_sql, apply_edit, cancel_query, connect, delete_connection,
    disconnect, export_result, export_table, generate_ddl, get_ai_config, get_result_window,
    get_table_window,
    doc_collections, doc_find, explain_query, export_connections, foreign_keys, import_connections,
    import_rows, kv_get, kv_scan, list_connections, list_objects, list_saved_queries,
    local_llm_available, open_table, parse_import, primary_key, profile_column, query_history,
    run_query, run_script, save_ai_config, save_connection, save_query, delete_saved_query,
    test_connection, ConnState,
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
            cancel_query,
            get_result_window,
            primary_key,
            apply_edit,
            export_result,
            export_table,
            run_script,
            generate_ddl,
            list_saved_queries,
            save_query,
            delete_saved_query,
            query_history,
            parse_import,
            import_rows,
            profile_column,
            get_ai_config,
            save_ai_config,
            ai_egress_preview,
            ai_generate_sql,
            explain_query,
            foreign_keys,
            local_llm_available,
            kv_scan,
            kv_get,
            doc_collections,
            doc_find,
            export_connections,
            import_connections,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
