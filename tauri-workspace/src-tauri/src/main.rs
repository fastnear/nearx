#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use serde_json::{json, Value};
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(feature = "e2e")]
mod test_api;

#[derive(Default, Clone)]
struct PendingLinks(Arc<Mutex<Vec<String>>>);

#[derive(Debug, Serialize)]
struct RuntimeConfig {
    near_node_url: String,
    fastnear_api_url: String,
    fastnear_auth_token: Option<String>,
    fastnear_auth_token_source: String,
    broker_available: bool,
}

fn default_near_node_url() -> String {
    env::var("NEAR_NODE_URL").unwrap_or_else(|_| "https://rpc.mainnet.fastnear.com/".to_string())
}

fn default_fastnear_api_url() -> String {
    env::var("FASTNEAR_API_URL").unwrap_or_else(|_| "https://tx.main.fastnear.com".to_string())
}

#[cfg(unix)]
fn nearxd_socket_path() -> PathBuf {
    if let Ok(path) = env::var("NEARXD_SOCKET_PATH") {
        let p = PathBuf::from(path.trim());
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    env::temp_dir().join("nearxd.sock")
}

#[cfg(unix)]
fn nearxd_request(method: &str, params: Value) -> Result<Value, String> {
    let socket_path = nearxd_socket_path();
    let mut stream = UnixStream::connect(&socket_path)
        .map_err(|e| format!("connect nearxd {}: {e}", socket_path.display()))?;

    let req = json!({
        "id": "tauri",
        "method": method,
        "params": params,
    });

    let payload = serde_json::to_string(&req).map_err(|e| format!("encode nearxd request: {e}"))?;
    stream
        .write_all(payload.as_bytes())
        .map_err(|e| format!("write nearxd request: {e}"))?;
    stream
        .write_all(b"\n")
        .map_err(|e| format!("write nearxd newline: {e}"))?;
    stream
        .flush()
        .map_err(|e| format!("flush nearxd request: {e}"))?;

    let mut line = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut line)
        .map_err(|e| format!("read nearxd response: {e}"))?;

    if line.trim().is_empty() {
        return Err("nearxd returned empty response".to_string());
    }

    let resp: Value =
        serde_json::from_str(line.trim()).map_err(|e| format!("decode nearxd response: {e}"))?;

    if resp.get("ok").and_then(Value::as_bool) == Some(true) {
        return Ok(resp.get("result").cloned().unwrap_or_else(|| json!({})));
    }

    let msg = resp
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("nearxd rejected request")
        .to_string();
    Err(msg)
}

#[cfg(not(unix))]
fn nearxd_request(_method: &str, _params: Value) -> Result<Value, String> {
    Err("nearxd unix socket transport unavailable on this platform".to_string())
}

pub(crate) fn canonicalize_deep_link(raw: &str) -> Option<String> {
    let s = raw.trim();
    if !(s.starts_with("nearx:") || s.starts_with("near:") || s.contains("://")) {
        return None;
    }

    if let Ok(result) = nearxd_request("parse_deep_link", json!({ "url": raw })) {
        if let Some(canonical) = result.get("canonical_url").and_then(Value::as_str) {
            return Some(canonical.to_string());
        }
    }

    // Fallback when nearxd is not running: only accept strict v1 forms.
    if s.starts_with("nearx://v1/") {
        return Some(s.to_string());
    }
    if s.starts_with("near://v1/") {
        return Some(s.replacen("near://", "nearx://", 1));
    }

    None
}

#[tauri::command]
async fn request_user_presence(reason: Option<String>) -> Result<Value, String> {
    let reason_str = reason.unwrap_or_else(|| "NEARx authentication".to_string());
    log::info!("[request_user_presence] calling nearxd with reason={reason_str:?}");
    match nearxd_request(
        "request_user_presence",
        json!({
            "reason": reason_str,
            "allow_fallback": true
        }),
    ) {
        Ok(val) => {
            log::info!("[request_user_presence] success: {val}");
            Ok(val)
        }
        Err(e) => {
            log::error!("[request_user_presence] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn list_near_credentials(network: String) -> Result<Value, String> {
    log::info!("[list_near_credentials] network={network}");
    match nearxd_request("list_near_credentials", json!({ "network": network })) {
        Ok(val) => Ok(val),
        Err(e) => {
            log::error!("[list_near_credentials] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn import_near_credentials(params: Value) -> Result<Value, String> {
    log::info!("[import_near_credentials] params: {params}");
    match nearxd_request("import_near_credentials", params) {
        Ok(val) => {
            log::info!("[import_near_credentials] success: {val}");
            Ok(val)
        }
        Err(e) => {
            log::error!("[import_near_credentials] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn sign_transaction(params: Value) -> Result<Value, String> {
    log::info!("[sign_transaction] params: {params}");
    match nearxd_request("sign_transaction", params) {
        Ok(val) => {
            // Don't log success payload (may contain sensitive data)
            log::info!("[sign_transaction] success (tx_hash={})", val.get("tx_hash").and_then(|v| v.as_str()).unwrap_or("?"));
            Ok(val)
        }
        Err(e) => {
            log::error!("[sign_transaction] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
fn open_external(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_runtime_config() -> RuntimeConfig {
    let env_token = env::var("FASTNEAR_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            env::var("FASTNEAR_AUTH_TOKEN")
                .ok()
                .filter(|s| !s.is_empty())
        });
    let mut cfg = RuntimeConfig {
        near_node_url: default_near_node_url(),
        fastnear_api_url: default_fastnear_api_url(),
        fastnear_auth_token: env_token.clone(),
        fastnear_auth_token_source: if env_token.is_some() {
            "env_api_key_or_auth_token".to_string()
        } else {
            "none".to_string()
        },
        broker_available: false,
    };

    if let Ok(result) = nearxd_request("get_runtime_config", json!({ "include_token": true })) {
        cfg.broker_available = true;

        if let Some(v) = result.get("near_node_url").and_then(Value::as_str) {
            cfg.near_node_url = v.to_string();
        }
        if let Some(v) = result.get("fastnear_api_url").and_then(Value::as_str) {
            cfg.fastnear_api_url = v.to_string();
        }

        cfg.fastnear_auth_token = result
            .get("fastnear_auth_token")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .or(cfg.fastnear_auth_token);

        if let Some(src) = result
            .get("fastnear_auth_token_source")
            .and_then(Value::as_str)
        {
            cfg.fastnear_auth_token_source = src.to_string();
        }
    }

    cfg
}

fn main() {
    let pending = PendingLinks::default();

    let mut builder = tauri::Builder::default()
        .manage(pending.clone())
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_single_instance::init({
            move |app, argv, _cwd| {
                for arg in argv {
                    if let Some(canonical) = canonicalize_deep_link(&arg) {
                        let _ = app.emit("nearx://open", canonical);
                    }
                }
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init());

    #[cfg(feature = "e2e")]
    {
        builder = builder.invoke_handler(tauri::generate_handler![
            open_external,
            get_runtime_config,
            request_user_presence,
            list_near_credentials,
            import_near_credentials,
            sign_transaction,
            test_api::nearx_test_emit_deeplink,
            test_api::nearx_test_roundtrip_deeplink,
            test_api::nearx_test_get_last_route,
            test_api::nearx_test_clear_storage
        ]);
    }

    #[cfg(not(feature = "e2e"))]
    {
        builder =
            builder.invoke_handler(tauri::generate_handler![open_external, get_runtime_config, request_user_presence, list_near_credentials, import_near_credentials, sign_transaction]);
    }

    builder
        .setup(move |app| {
            let app_handle = app.handle().clone();

            #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
            {
                use tauri_plugin_deep_link::DeepLinkExt;

                #[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
                {
                    #[cfg(feature = "e2e")]
                    {
                        log::info!("E2E mode: skipping deep-link scheme registration");
                    }
                    #[cfg(not(feature = "e2e"))]
                    app.deep_link().register_all()?;
                }

                app.deep_link().on_open_url(move |event| {
                    for url in event.urls() {
                        if let Some(canonical) = canonicalize_deep_link(&url.to_string()) {
                            let _ = app_handle.emit("nearx://open", canonical);
                        }
                    }
                });

                if let Some(urls) = app.deep_link().get_current()? {
                    for url in urls {
                        if let Some(canonical) = canonicalize_deep_link(&url.to_string()) {
                            pending.0.lock().unwrap().push(canonical);
                        }
                    }
                }
            }

            if let Some(win) = app.get_webview_window("main") {
                let mut q = pending.0.lock().unwrap();
                for url in q.drain(..) {
                    let _ = win.emit("nearx://open", url);
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("NEARx Tauri failed");
}
