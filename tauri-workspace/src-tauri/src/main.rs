#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use nearx_broker_ipc::BrokerEndpoint;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use tauri_plugin_shell::ShellExt;

#[cfg(feature = "e2e")]
mod test_api;

#[derive(Default, Clone)]
struct PendingLinks(Arc<Mutex<Vec<String>>>);

struct SidecarChild(Mutex<Option<tauri_plugin_shell::process::CommandChild>>);

impl Drop for SidecarChild {
    fn drop(&mut self) {
        if let Some(child) = self.0.lock().unwrap().take() {
            let _ = child.kill();
            log::info!("nearxd sidecar terminated");
        }
    }
}

#[derive(Debug, Serialize)]
struct RuntimeConfig {
    near_node_url: String,
    fastnear_api_url: String,
    fastnear_auth_token: Option<String>,
    fastnear_auth_token_source: String,
    broker_available: bool,
}

#[derive(Debug, Deserialize)]
struct FastnearJsonRequest {
    url: String,
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    headers: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    body: Option<Value>,
    #[serde(default = "default_true")]
    include_api_key: bool,
}

#[derive(Debug, Serialize)]
struct FastnearJsonResponse {
    url: String,
    status: u16,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    body: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
}

fn default_near_node_url() -> String {
    env::var("NEAR_NODE_URL").unwrap_or_else(|_| "https://rpc.mainnet.fastnear.com/".to_string())
}

fn default_fastnear_api_url() -> String {
    env::var("FASTNEAR_API_URL").unwrap_or_else(|_| "https://tx.main.fastnear.com".to_string())
}

fn default_true() -> bool {
    true
}

fn append_api_key_query(raw_url: &str, api_key: Option<&str>) -> String {
    let Some(api_key) = api_key.map(str::trim).filter(|value| !value.is_empty()) else {
        return raw_url.to_string();
    };

    let Ok(mut url) = url::Url::parse(raw_url) else {
        return raw_url.to_string();
    };

    if url.query_pairs().any(|(key, _)| key == "apiKey") {
        return url.to_string();
    }

    url.query_pairs_mut().append_pair("apiKey", api_key);
    url.to_string()
}

fn redact_url_for_logs(raw_url: &str) -> String {
    let Ok(mut url) = url::Url::parse(raw_url) else {
        return raw_url.to_string();
    };

    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            if key == "apiKey" {
                (key.into_owned(), "REDACTED".to_string())
            } else {
                (key.into_owned(), value.into_owned())
            }
        })
        .collect();

    url.set_query(None);
    if !pairs.is_empty() {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in pairs {
            query.append_pair(&key, &value);
        }
        url.set_query(Some(&query.finish()));
    }

    url.to_string()
}

fn truncate_for_log(raw: &str, max_len: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.len() <= max_len {
        return trimmed.to_string();
    }

    format!("{}...", &trimmed[..max_len])
}

fn nearxd_endpoint() -> BrokerEndpoint {
    BrokerEndpoint::from_env()
}

fn nearxd_request(method: &str, params: Value) -> Result<Value, String> {
    let endpoint = nearxd_endpoint();
    let mut stream = endpoint
        .connect()
        .map_err(|e| format!("connect nearxd {}: {e}", endpoint.display()))?;

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
    let code = resp
        .pointer("/error/code")
        .and_then(Value::as_str)
        .unwrap_or("ERR_BROKER");
    Err(format!("{code}: {msg}"))
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
async fn list_near_signing_accounts(params: Value) -> Result<Value, String> {
    log::info!("[list_near_signing_accounts] params={params}");
    match nearxd_request("list_near_signing_accounts", params) {
        Ok(val) => Ok(val),
        Err(e) => {
            log::error!("[list_near_signing_accounts] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn list_near_signing_keys(params: Value) -> Result<Value, String> {
    log::info!("[list_near_signing_keys] params={params}");
    match nearxd_request("list_near_signing_keys", params) {
        Ok(val) => Ok(val),
        Err(e) => {
            log::error!("[list_near_signing_keys] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn import_near_signing_keys(params: Value) -> Result<Value, String> {
    log::info!("[import_near_signing_keys] params={params}");
    match nearxd_request("import_near_signing_keys", params) {
        Ok(val) => {
            log::info!("[import_near_signing_keys] success: {val}");
            Ok(val)
        }
        Err(e) => {
            log::error!("[import_near_signing_keys] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn reprotect_near_signing_key(params: Value) -> Result<Value, String> {
    log::info!("[reprotect_near_signing_key] params={params}");
    match nearxd_request("reprotect_near_signing_key", params) {
        Ok(val) => {
            log::info!("[reprotect_near_signing_key] success: {val}");
            Ok(val)
        }
        Err(e) => {
            log::error!("[reprotect_near_signing_key] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn set_signing_key_label(params: Value) -> Result<Value, String> {
    log::info!("[set_signing_key_label] params={params}");
    match nearxd_request("set_signing_key_label", params) {
        Ok(val) => Ok(val),
        Err(e) => {
            log::error!("[set_signing_key_label] error: {e}");
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
async fn list_staking_watchlist(params: Value) -> Result<Value, String> {
    log::info!("[list_staking_watchlist] params={params}");
    match nearxd_request("list_staking_watchlist", params) {
        Ok(val) => Ok(val),
        Err(e) => {
            log::error!("[list_staking_watchlist] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn add_staking_watchlist_account(params: Value) -> Result<Value, String> {
    log::info!("[add_staking_watchlist_account] params={params}");
    match nearxd_request("add_staking_watchlist_account", params) {
        Ok(val) => Ok(val),
        Err(e) => {
            log::error!("[add_staking_watchlist_account] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn remove_staking_watchlist_account(params: Value) -> Result<Value, String> {
    log::info!("[remove_staking_watchlist_account] params={params}");
    match nearxd_request("remove_staking_watchlist_account", params) {
        Ok(val) => Ok(val),
        Err(e) => {
            log::error!("[remove_staking_watchlist_account] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn connect_hardware_wallet(params: Value) -> Result<Value, String> {
    log::info!("[connect_hardware_wallet] params={params}");
    match nearxd_request("connect_hardware_wallet", params) {
        Ok(val) => Ok(val),
        Err(e) => {
            log::error!("[connect_hardware_wallet] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn get_preferences() -> Result<Value, String> {
    log::info!("[get_preferences]");
    match nearxd_request("get_preferences", json!({})) {
        Ok(val) => Ok(val),
        Err(e) => {
            log::error!("[get_preferences] error: {e}");
            Err(e)
        }
    }
}

#[tauri::command]
async fn set_preferences(params: Value) -> Result<Value, String> {
    log::info!("[set_preferences] params={params}");
    match nearxd_request("set_preferences", params) {
        Ok(val) => Ok(val),
        Err(e) => {
            log::error!("[set_preferences] error: {e}");
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

#[derive(Serialize)]
struct WasmFileResult {
    file_name: String,
    file_size: usize,
    code_base64: String,
    directory: Option<String>,
}

#[tauri::command]
async fn pick_wasm_file(default_dir: Option<String>) -> Result<Option<WasmFileResult>, String> {
    let mut dialog = rfd::AsyncFileDialog::new()
        .add_filter("WASM", &["wasm"])
        .set_title("Select WASM contract file");
    if let Some(dir) = &default_dir {
        dialog = dialog.set_directory(dir);
    }
    let Some(handle) = dialog.pick_file().await else {
        return Ok(None);
    };
    let data = handle.read().await;
    use base64::Engine;
    let code_base64 = base64::engine::general_purpose::STANDARD.encode(&data);
    let path = handle.path().to_path_buf();
    Ok(Some(WasmFileResult {
        file_name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        file_size: data.len(),
        code_base64,
        directory: path.parent().map(|p| p.to_string_lossy().to_string()),
    }))
}

#[tauri::command]
async fn get_signing_capabilities() -> Result<Value, String> {
    log::info!("[get_signing_capabilities]");
    match nearxd_request("get_signing_capabilities", json!({})) {
        Ok(val) => Ok(val),
        Err(e) => {
            log::error!("[get_signing_capabilities] error: {e}");
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

#[tauri::command]
async fn fetch_fastnear_json(params: Value) -> Result<FastnearJsonResponse, String> {
    let params: FastnearJsonRequest = serde_json::from_value(params)
        .map_err(|e| format!("invalid fetch_fastnear_json params: {e}"))?;

    let method = params
        .method
        .as_deref()
        .unwrap_or(if params.body.is_some() { "POST" } else { "GET" })
        .trim()
        .to_ascii_uppercase();
    let request_method =
        reqwest::Method::from_bytes(method.as_bytes()).map_err(|e| format!("invalid method: {e}"))?;

    let runtime = get_runtime_config();
    let request_url = if params.include_api_key {
        append_api_key_query(&params.url, runtime.fastnear_auth_token.as_deref())
    } else {
        params.url.clone()
    };
    let redacted_url = redact_url_for_logs(&request_url);

    log::info!(
        "[fetch_fastnear_json] method={} url={} include_api_key={} token_source={}",
        method,
        redacted_url,
        params.include_api_key,
        runtime.fastnear_auth_token_source
    );

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| format!("build HTTP client: {e}"))?;
    let mut request = client.request(request_method, &request_url);

    for (name, value) in &params.headers {
        request = request.header(name.as_str(), value.as_str());
    }

    if let Some(body) = params.body {
        request = request.json(&body);
    }

    let response = request.send().await.map_err(|e| {
        log::error!(
            "[fetch_fastnear_json] request error method={} url={} error={}",
            method,
            redacted_url,
            e
        );
        format!("request failed: {e}")
    })?;

    let status = response.status();
    let text = response.text().await.map_err(|e| {
        log::error!(
            "[fetch_fastnear_json] response-read error method={} url={} status={} error={}",
            method,
            redacted_url,
            status.as_u16(),
            e
        );
        format!("read response body: {e}")
    })?;
    let parsed_body = serde_json::from_str::<Value>(&text).ok();
    let has_parsed_body = parsed_body.is_some();

    if status.is_success() {
        log::info!(
            "[fetch_fastnear_json] success method={} url={} status={}",
            method,
            redacted_url,
            status.as_u16()
        );
    } else {
        log::error!(
            "[fetch_fastnear_json] failure method={} url={} status={} body={}",
            method,
            redacted_url,
            status.as_u16(),
            truncate_for_log(&text, 240)
        );
    }

    Ok(FastnearJsonResponse {
        url: redacted_url,
        status: status.as_u16(),
        ok: status.is_success(),
        body: parsed_body,
        text: if text.trim().is_empty() || (status.is_success() && has_parsed_body) {
            None
        } else {
            Some(text)
        },
    })
}

fn main() {
    let pending = PendingLinks::default();

    let mut builder = tauri::Builder::default()
        .manage(pending.clone())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level_for("reqwest", log::LevelFilter::Info)
                .build(),
        )
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
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_shell::init());

    #[cfg(feature = "e2e")]
    {
        builder = builder.invoke_handler(tauri::generate_handler![
            open_external,
            get_runtime_config,
            fetch_fastnear_json,
            request_user_presence,
            list_near_credentials,
            list_staking_watchlist,
            add_staking_watchlist_account,
            remove_staking_watchlist_account,
            connect_hardware_wallet,
            get_signing_capabilities,
            get_preferences,
            set_preferences,
            list_near_signing_accounts,
            list_near_signing_keys,
            import_near_signing_keys,
            reprotect_near_signing_key,
            set_signing_key_label,
            import_near_credentials,
            sign_transaction,
            pick_wasm_file,
            test_api::nearx_test_emit_deeplink,
            test_api::nearx_test_roundtrip_deeplink,
            test_api::nearx_test_get_last_route,
            test_api::nearx_test_clear_storage
        ]);
    }

    #[cfg(not(feature = "e2e"))]
    {
        builder =
            builder.invoke_handler(tauri::generate_handler![
                open_external,
                get_runtime_config,
                fetch_fastnear_json,
                request_user_presence,
                list_near_credentials,
                list_staking_watchlist,
                add_staking_watchlist_account,
                remove_staking_watchlist_account,
                connect_hardware_wallet,
                get_signing_capabilities,
                get_preferences,
                set_preferences,
                list_near_signing_accounts,
                list_near_signing_keys,
                import_near_signing_keys,
                reprotect_near_signing_key,
                set_signing_key_label,
                import_near_credentials,
                sign_transaction,
                pick_wasm_file
            ]);
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

            // Spawn nearxd sidecar if no standalone instance is already running
            #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
            {
                let endpoint = nearxd_endpoint();
                let standalone_running = endpoint.connect().is_ok();

                if standalone_running {
                    log::info!(
                        "nearxd already running at {}, skipping sidecar",
                        endpoint.display()
                    );
                } else {
                    match app.shell().sidecar("nearxd") {
                        Ok(cmd) => {
                            let sidecar_endpoint =
                                BrokerEndpoint::tauri_sidecar_endpoint(std::process::id());
                            let mut cmd = cmd;
                            for (key, value) in sidecar_endpoint.export_env_vars() {
                                cmd = cmd.env(key, value);
                            }

                            match cmd.spawn() {
                                Ok((_rx, child)) => {
                                    log::info!(
                                        "nearxd sidecar spawned ({})",
                                        sidecar_endpoint.display()
                                    );
                                    // Point our broker requests at the sidecar endpoint.
                                    #[allow(deprecated)]
                                    unsafe {
                                        for (key, value) in sidecar_endpoint.export_env_vars() {
                                            env::set_var(key, value);
                                        }
                                    }
                                    app.manage(SidecarChild(Mutex::new(Some(child))));

                                    // Wait briefly for the sidecar to start listening
                                    for i in 0..20 {
                                        if sidecar_endpoint.connect().is_ok() {
                                            log::info!("nearxd sidecar ready after {}ms", i * 50);
                                            break;
                                        }
                                        std::thread::sleep(std::time::Duration::from_millis(50));
                                    }
                                }
                                Err(e) => log::warn!("nearxd sidecar spawn failed: {e}"),
                            }
                        }
                        Err(e) => log::info!("nearxd sidecar not bundled: {e}"),
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
