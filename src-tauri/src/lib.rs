// TableIQ — Tauri entry.
pub mod ai;
pub mod appdir;
pub mod applog;
pub mod bench;
pub mod connections;
pub mod drivers;
pub mod ipc;
pub mod library;
pub mod plugins;
pub mod query;
pub mod sql_log;
pub mod ssh;

/// Lock a mutex, recovering from poisoning instead of panicking. A panic while one lock is
/// held would otherwise poison it and take down every later caller that touches it.
pub(crate) fn lk<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

use ipc::connect::{
    ai_egress_preview, ai_explain_sql, ai_fix_sql, ai_generate_sql, ai_optimize_sql, apply_edit,
    cancel_all, cancel_query, connect, count_rows_exact, delete_connection, delete_row, describe_table,
    estimate_rows,
    disconnect, diff_results, diff_schemas, export_result, export_table, generate_ddl,
    get_ai_config, get_result_window, get_table_window,
    doc_collections, doc_find, explain_query, export_connections, foreign_keys, import_connections,
    apply_parquet_import, import_rows, insert_row, install_sample_plugin, kv_get, kv_scan,
    list_connections, list_databases, list_objects, list_plugins, list_saved_queries,
    local_llm_available, log_client_error, open_table, parse_import, parse_parquet_preview, primary_key,
    profile_column, query_history, query_rows, read_onboarding_state, run_plugin_on_result, run_query,
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
        count_rows_exact,
        get_table_window,
        run_query,
        cancel_query,
        cancel_all,
        get_result_window,
        primary_key,
        apply_edit,
        delete_row,
        export_result,
        export_table,
        run_script,
        query_rows,
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
        kv_get,
        doc_collections,
        doc_find,
        export_connections,
        import_connections,
        read_onboarding_state,
        write_onboarding_state,
        write_text_file,
        sql_log,
        clear_sql_log,
    ]
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    applog::init();
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(ConnState::new())
        .invoke_handler(invoke_handler())
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
