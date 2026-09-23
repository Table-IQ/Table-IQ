//! Dev-only QA harness: serves the real `#[tauri::command]` handlers over HTTP so the React UI,
//! running in a normal browser (`VITE_IPC_BRIDGE=1 pnpm dev`), talks to real Rust and real DBs.
//! `POST /invoke/<cmd>` with the same JSON args object JS `invoke` sends (camelCase keys).
//! Run: `TIQ_CONFIG_DIR=/tmp/tiq-qa cargo run --features ipc-bridge --example ipc_bridge`

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};

use tauri::ipc::{CallbackFn, InvokeBody, InvokeResponseBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY};
use tauri::webview::InvokeRequest;
use tauri::WebviewWindow;

/// Browsers always send `Origin` on POST; only the vite dev server may call us, which blocks
/// CSRF / DNS rebinding from any other page. curl sends no Origin and is allowed.
const ALLOWED_ORIGINS: [&str; 2] = ["http://localhost:1420", "http://127.0.0.1:1420"];

type Reply = (&'static str, &'static str, Vec<u8>);

fn main() {
    tableiq_lib::applog::init();
    let mut ctx = mock_context(noop_assets());
    // Onboarding uses app_config_dir() = config_dir().join(identifier). An absolute identifier
    // replaces that path, so onboarding also lands in app_dir() and honours TIQ_CONFIG_DIR.
    ctx.config_mut().identifier = tableiq_lib::appdir::app_dir().to_string_lossy().into_owned();
    let app = mock_builder()
        .manage(tableiq_lib::ipc::connect::ConnState::new())
        .invoke_handler(tableiq_lib::invoke_handler())
        .build(ctx)
        .expect("build mock app");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("build mock webview");

    let port = std::env::var("TIQ_BRIDGE_PORT").unwrap_or_else(|_| "1430".into());
    let listener = TcpListener::bind(format!("127.0.0.1:{port}")).expect("bind bridge port");
    println!("ipc_bridge listening on http://127.0.0.1:{port}");
    for stream in listener.incoming().flatten() {
        let webview = webview.clone();
        std::thread::spawn(move || {
            let (status, content_type, body) = route(&stream, &webview);
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let mut out = &stream;
            let _ = out.write_all(head.as_bytes()).and_then(|_| out.write_all(&body));
        });
    }
}

fn err(status: &'static str, msg: &str) -> Reply {
    (status, "application/json", serde_json::Value::from(msg).to_string().into_bytes())
}

// ponytail: minimal HTTP/1.1 — Content-Length bodies only (no chunked), no keep-alive, no body
// cap. Fine for a localhost dev harness; upgrade to a real HTTP server crate if it needs more.
fn route(stream: &TcpStream, webview: &WebviewWindow<MockRuntime>) -> Reply {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    let _ = reader.read_line(&mut request_line);
    let (mut len, mut origin_ok) = (0usize, true);
    for line in reader.by_ref().lines().map_while(Result::ok) {
        if line.is_empty() {
            break;
        }
        let Some((k, v)) = line.split_once(':') else { continue };
        match k.trim().to_ascii_lowercase().as_str() {
            "content-length" => len = v.trim().parse().unwrap_or(0),
            "origin" => origin_ok = ALLOWED_ORIGINS.contains(&v.trim()),
            _ => {}
        }
    }
    let mut body = vec![0u8; len];
    if reader.read_exact(&mut body).is_err() {
        return err("400 Bad Request", "truncated body");
    }

    let mut parts = request_line.split_whitespace();
    let cmd = match (parts.next(), parts.next().and_then(|p| p.strip_prefix("/invoke/"))) {
        (Some("POST"), Some(cmd)) if !cmd.is_empty() => cmd.to_string(),
        _ => return err("404 Not Found", "use POST /invoke/<cmd>"),
    };
    if !origin_ok {
        return err("403 Forbidden", "origin not allowed");
    }
    let args = if body.is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(e) => return err("400 Bad Request", &e.to_string()),
        }
    };

    let request = InvokeRequest {
        cmd,
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().unwrap(),
        body: InvokeBody::Json(args),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    };
    // A sync command panics on this thread; an async one drops the responder, so
    // get_ipc_response's recv().expect() panics here instead. Either way we answer 500.
    match catch_unwind(AssertUnwindSafe(|| get_ipc_response(webview, request))) {
        Ok(Ok(InvokeResponseBody::Json(s))) => ("200 OK", "application/json", s.into_bytes()),
        Ok(Ok(InvokeResponseBody::Raw(b))) => ("200 OK", "application/octet-stream", b),
        Ok(Err(v)) => ("400 Bad Request", "application/json", v.to_string().into_bytes()),
        Err(p) => {
            let msg = p
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            let body = serde_json::json!({ "panic": msg }).to_string().into_bytes();
            ("500 Internal Server Error", "application/json", body)
        }
    }
}
