//! Rust→UI window transports (plan: benchmark three approaches and pick by data).
//!
//! 1. `get_window_response` — raw bytes via `tauri::ipc::Response` (no base64 JSON tax).
//! 2. `get_window_channel`  — raw bytes streamed over a `tauri::ipc::Channel`.
//! 3. custom `tiqwin://` URI-scheme protocol — registered in `lib.rs::run`.
//!
//! All three return the same Arrow IPC window bytes; the frontend times each.

use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody, Response};
use tauri::State;

use crate::query::window::WindowSource;

/// Shared app state: the materialized window source (synth 1M rows).
pub struct AppState {
    pub materialize: Arc<WindowSource>,
}

#[derive(Serialize)]
pub struct WindowMeta {
    pub total_rows: usize,
}

/// Grid setup: total row count of the active source.
#[tauri::command]
pub fn window_meta(state: State<AppState>) -> WindowMeta {
    WindowMeta {
        total_rows: state.materialize.total_rows(),
    }
}

/// Transport 1: raw bytes as a command response.
#[tauri::command]
pub fn get_window_response(state: State<AppState>, offset: usize, len: usize) -> Response {
    let bytes = state.materialize.window_ipc(offset, len).unwrap_or_default();
    Response::new(bytes)
}

/// Transport 2: raw bytes over a channel.
#[tauri::command]
pub async fn get_window_channel(
    state: State<'_, AppState>,
    offset: usize,
    len: usize,
    on_window: Channel<InvokeResponseBody>,
) -> Result<(), String> {
    let bytes = state
        .materialize
        .window_ipc(offset, len)
        .map_err(|e| e.to_string())?;
    on_window
        .send(InvokeResponseBody::Raw(bytes))
        .map_err(|e| e.to_string())
}
