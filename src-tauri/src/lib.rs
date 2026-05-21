// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
pub mod bench;
pub mod drivers;
pub mod ipc;
pub mod query;

use std::sync::Arc;

use ipc::transport::{get_window_channel, get_window_response, window_meta, AppState};
use query::synth::synth_batch;
use query::window::WindowSource;

/// How many rows the materialized GUI source holds. The gate run uses 1,000,000;
/// override with TIQ_ROWS for faster dev iteration (UI metrics are independent of total).
fn gui_rows() -> usize {
    std::env::var("TIQ_ROWS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let rows = gui_rows();
    let batch = synth_batch(0, rows, 100);
    let materialize = Arc::new(WindowSource::materialize(Arc::new(batch)));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState { materialize })
        // Transport 3: custom URI-scheme protocol — tiqwin://localhost/window/<offset>/<len>
        .register_asynchronous_uri_scheme_protocol("tiqwin", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();
            let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
            let offset = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0usize);
            let len = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0usize);
            let bytes = {
                let state = tauri::Manager::state::<AppState>(&app);
                state.materialize.window_ipc(offset, len).unwrap_or_default()
            };
            let resp = tauri::http::Response::builder()
                .header("Content-Type", "application/vnd.apache.arrow.stream")
                .header("Access-Control-Allow-Origin", "*")
                .body(std::borrow::Cow::<'static, [u8]>::Owned(bytes))
                .unwrap();
            responder.respond(resp);
        })
        .invoke_handler(tauri::generate_handler![
            window_meta,
            get_window_response,
            get_window_channel
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
