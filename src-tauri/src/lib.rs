// TableIQ — Tauri entry.
pub mod ai;
pub mod appdir;
pub mod bench;
pub mod connections;
pub mod drivers;
pub mod ipc;
pub mod library;
pub mod plugins;
pub mod query;
pub mod ssh;

use ipc::connect::{
    ai_egress_preview, ai_explain_sql, ai_fix_sql, ai_generate_sql, ai_optimize_sql, apply_edit,
    cancel_query, connect, delete_connection, delete_row,
    disconnect, diff_results, diff_schemas, export_result, export_table, generate_ddl,
    get_ai_config, get_result_window, get_table_window,
    doc_collections, doc_find, explain_query, export_connections, foreign_keys, import_connections,
    apply_parquet_import, import_rows, insert_row, install_sample_plugin, kv_get, kv_scan,
    list_connections, list_databases, list_objects, list_plugins, list_saved_queries,
    local_llm_available, open_table, parse_import, parse_parquet_preview, primary_key,
    profile_column, query_history, read_onboarding_state, run_plugin_on_result, run_query,
    run_script, save_ai_config, save_connection, save_query, delete_saved_query, switch_database,
    test_connection, write_onboarding_state, ConnState,
};

/// Every IPC command. Generic over the runtime so the QA bridge (`examples/ipc_bridge.rs`)
/// can mount the exact same handlers on tauri's MockRuntime.
pub fn invoke_handler<R: tauri::Runtime>(
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        list_connections,
        save_connection,
        delete_connection,
        test_connection,
        connect,
        disconnect,
        list_databases,
        switch_database,
        list_objects,
        open_table,
        get_table_window,
        run_query,
        cancel_query,
        get_result_window,
        primary_key,
        apply_edit,
        delete_row,
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
        insert_row,
        parse_parquet_preview,
        apply_parquet_import,
        profile_column,
        diff_schemas,
        diff_results,
        list_plugins,
        install_sample_plugin,
        run_plugin_on_result,
        get_ai_config,
        save_ai_config,
        ai_egress_preview,
        ai_generate_sql,
        ai_explain_sql,
        ai_optimize_sql,
        ai_fix_sql,
        explain_query,
        foreign_keys,
        local_llm_available,
        kv_scan,
        kv_get,
        doc_collections,
        doc_find,
        export_connections,
        import_connections,
        read_onboarding_state,
        write_onboarding_state,
    ]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(ConnState::new())
        .invoke_handler(invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
