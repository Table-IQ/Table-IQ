// TableIQ — Tauri entry.
// `#[async_trait]` puts its own `#[must_use]` on every async method; Rust 1.99's clippy reads that as a
// duplicate. Drop once async_trait stops emitting it.
#![allow(clippy::double_must_use)]
pub mod ai;
pub mod appdir;
pub mod applog;
pub mod backup;
pub mod bench;
pub mod connections;
pub mod drivers;
pub mod ipc;
pub mod library;
pub mod mcp;
// The native menu is macOS-only; the pure item table and its tests build everywhere.
#[cfg(any(target_os = "macos", test))]
mod menu;
pub mod plugins;
pub mod profiles;
pub mod query;
pub mod sql_log;
pub mod ssh;

/// Lock a mutex, recovering from poisoning instead of panicking. A panic while one lock is
/// held would otherwise poison it and take down every later caller that touches it.
pub(crate) fn lk<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

use ipc::connect::{
    ai_egress_preview, ai_explain_sql, ai_fix_sql, ai_generate_sql, ai_optimize_sql, apply_changes, apply_edit,
    backup_database, backup_job, cancel_backup_job, detect_backup_tools, restore_database,
    cancel_all, cancel_query, connect, count_rows_exact, delete_connection, delete_row, describe_table,
    estimate_rows, table_stats,
    disconnect, diff_results, diff_schemas, export_result, export_table, generate_ddl,
    get_ai_config, get_result_window, get_table_window,
    doc_aggregate, doc_apply_changes, doc_collections, doc_count, doc_create_index, doc_drop_index, doc_find,
    doc_indexes, doc_mapping, explain_query, export_connections, foreign_keys, import_connections,
    apply_parquet_import, import_rows, insert_row, install_sample_plugin, kv_apply_changes, kv_command, kv_dbsize, kv_get, kv_read, kv_scan, kv_scan_page,
    list_connections, list_databases, list_objects, list_plugins, list_saved_queries,
    local_llm_available, log_client_error, open_table, parse_import, parse_parquet_preview, primary_key,
    profile_column, query_history, query_rows, validate_widget_sql, read_onboarding_state, run_plugin_on_result, run_query,
    run_script, save_ai_config, save_connection, save_query, delete_saved_query, sql_log,
    clear_sql_log, switch_database, test_connection, write_onboarding_state, write_text_file, ConnState,
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
        estimate_rows,
        table_stats,
        count_rows_exact,
        get_table_window,
        run_query,
        cancel_query,
        cancel_all,
        get_result_window,
        primary_key,
        apply_edit,
        delete_row,
        apply_changes,
        export_result,
        export_table,
        run_script,
        query_rows,
        validate_widget_sql,
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
        describe_table,
        local_llm_available,
        log_client_error,
        kv_scan,
        kv_scan_page,
        kv_get,
        kv_read,
        kv_apply_changes,
        kv_command,
        kv_dbsize,
        doc_collections,
        doc_find,
        doc_count,
        doc_apply_changes,
        doc_indexes,
        doc_create_index,
        doc_drop_index,
        doc_aggregate,
        doc_mapping,
        export_connections,
        import_connections,
        read_onboarding_state,
        write_onboarding_state,
        write_text_file,
        sql_log,
        clear_sql_log,
        backup_database,
        backup_job,
        cancel_backup_job,
        detect_backup_tools,
        restore_database,
        mcp::mcp_get_config,
        mcp::mcp_role_warnings,
        mcp::mcp_set_config,
        mcp::mcp_status,
        mcp::mcp_token,
        mcp::mcp_regenerate_token,
        mcp::mcp_activity,
    ]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    applog::init();
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(ConnState::new())
        .invoke_handler(invoke_handler());
    // Native View/Help menus (macOS only: `Builder::menu` becomes every window's menu, and
    // Windows/Linux have no designed menu bar). Items emit `tiq:menu` (src/lib/menuEvents.ts).
    #[cfg(target_os = "macos")]
    let builder = builder.menu(menu::build).on_menu_event(menu::on_event);
    builder
        // The MCP server starts with the app when enabled. The QA bridge doesn't run this;
        // Settings → MCP starts it there.
        .setup(|app| {
            tauri::async_runtime::spawn(mcp::autostart(app.handle().clone()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod lk_tests {
    use super::lk;
    use std::sync::Mutex;

    #[test]
    fn locks_normally_when_not_poisoned() {
        let m = Mutex::new(5);
        *lk(&m) += 1;
        assert_eq!(*lk(&m), 6);
    }

    /// The whole point of `lk`: every session-state mutex in `ipc::connect` (sessions,
    /// tunnels, results, …) is locked through it, so one panicking request must not poison
    /// the lock and turn every later `.unwrap()` caller into a crash too.
    #[test]
    fn recovers_a_poisoned_mutex_instead_of_panicking() {
        let m = Mutex::new(vec![1, 2, 3]);
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = m.lock().unwrap();
            panic!("simulated panic while holding the lock");
        }));
        assert!(res.is_err(), "the panic must have actually happened");
        assert!(m.is_poisoned(), "the mutex must actually be poisoned for this test to mean anything");

        // A plain `.lock().unwrap()` would panic here; `lk` must not.
        let guard = lk(&m);
        assert_eq!(*guard, vec![1, 2, 3], "the data survives poisoning intact");
    }
}
