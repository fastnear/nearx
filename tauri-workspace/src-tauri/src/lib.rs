#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod deeplink;

use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use tauri::{Emitter, Manager, Runtime};
use tauri_plugin_deep_link::DeepLinkExt;

#[cfg(desktop)]
use tauri::path::BaseDirectory;

static READY: OnceLock<Mutex<bool>> = OnceLock::new();
static QUEUE: OnceLock<Mutex<Vec<DeepLinkEvent>>> = OnceLock::new();

fn ready_get() -> bool {
    *READY.get_or_init(|| Mutex::new(false)).lock().unwrap()
}
fn ready_set(v: bool) {
    *READY.get_or_init(|| Mutex::new(false)).lock().unwrap() = v;
}
fn queue() -> &'static Mutex<Vec<DeepLinkEvent>> {
    QUEUE.get_or_init(|| Mutex::new(Vec::new()))
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "snake_case")]
pub struct DeepLinkEvent {
    pub url: String,
    pub scheme: String,
    pub host: String,
    pub path: Vec<String>,               // slug segments
    pub query: BTreeMap<String, String>, // ?k=v
}

#[tauri::command]
fn deeplink_frontend_ready() -> Vec<DeepLinkEvent> {
    log::info!(
        "Frontend ready - draining {} queued deep links",
        queue().lock().unwrap().len()
    );
    ready_set(true);
    queue().lock().unwrap().drain(..).collect()
}

fn normalize(raw: &str) -> Option<String> {
    log::info!("🔵 [NORMALIZE] ==================== START ====================");
    log::info!("🔵 [NORMALIZE] Input raw: {raw:?}");

    let s = raw.trim();
    log::info!("🔵 [NORMALIZE] After trim: {s:?}");

    if s.is_empty() {
        log::warn!("🔵 [NORMALIZE] Empty string after trim - returning None");
        return None;
    }

    // Accept: nearx://..., nearx:..., web+nearx:..., near:... (legacy)
    let mut s = s.replacen("web+nearx:", "nearx://", 1);
    s = s.replacen("web+near:", "nearx://", 1); // Legacy support
    log::info!("🔵 [NORMALIZE] After web+ replacement: {s:?}");

    // nearx:tx/abc  -> nearx://tx/abc
    // near:tx/abc   -> nearx://tx/abc (legacy)
    if let Some((sch, rest)) = s.split_once(':') {
        log::info!("🔵 [NORMALIZE] Split scheme: {sch:?}, rest: {rest:?}");
        if (sch.eq_ignore_ascii_case("nearx") || sch.eq_ignore_ascii_case("near"))
            && !rest.starts_with("//")
        {
            s = format!("nearx://{rest}");
            log::info!("🔵 [NORMALIZE] Added slashes: {s:?}");
        } else if sch.eq_ignore_ascii_case("near") && rest.starts_with("//") {
            // Legacy: near:// -> nearx://
            s = format!("nearx://{rest}");
            log::info!("🔵 [NORMALIZE] Converted legacy near:// to nearx://: {s:?}");
        }
    }

    // Must parse as a URL now
    match url::Url::parse(&s) {
        Ok(parsed) => {
            log::info!("🔵 [NORMALIZE] Successfully parsed as URL: {parsed:?}");
            log::info!("🔵 [NORMALIZE] Returning: {s:?}");
            log::info!("🔵 [NORMALIZE] ==================== END ====================");
            Some(s)
        }
        Err(e) => {
            log::warn!("🔵 [NORMALIZE] Failed to parse as URL: {e} - returning None");
            log::info!("🔵 [NORMALIZE] ==================== END ====================");
            None
        }
    }
}

fn parse_event(s: &str) -> Option<DeepLinkEvent> {
    log::info!("🟣 [PARSE-EVENT] ==================== START ====================");
    log::info!("🟣 [PARSE-EVENT] Input string: {s:?}");

    let url = match url::Url::parse(s) {
        Ok(u) => {
            log::info!("🟣 [PARSE-EVENT] Successfully parsed URL");
            log::info!("🟣 [PARSE-EVENT] Scheme: {:?}", u.scheme());
            log::info!("🟣 [PARSE-EVENT] Host: {:?}", u.host_str());
            log::info!("🟣 [PARSE-EVENT] Path: {:?}", u.path());
            log::info!("🟣 [PARSE-EVENT] Query: {:?}", u.query());
            u
        }
        Err(e) => {
            log::warn!("🟣 [PARSE-EVENT] Failed to parse URL: {e} - returning None");
            log::info!("🟣 [PARSE-EVENT] ==================== END ====================");
            return None;
        }
    };

    if url.scheme() != "nearx" {
        log::warn!(
            "🟣 [PARSE-EVENT] Wrong scheme: {:?} (expected 'nearx') - returning None",
            url.scheme()
        );
        log::info!("🟣 [PARSE-EVENT] ==================== END ====================");
        return None;
    }

    // host is your "resource" (tx/account/block/open/nearx)
    let host = url.host_str().unwrap_or_default().to_string();
    log::info!("🟣 [PARSE-EVENT] Extracted host: {host:?}");

    // collect non-empty path segments
    let path: Vec<String> = url
        .path_segments()
        .map(|segs| {
            let segments: Vec<String> = segs
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string())
                .collect();
            log::info!("🟣 [PARSE-EVENT] Extracted path segments: {segments:?}");
            segments
        })
        .unwrap_or_default();

    // map query pairs
    let mut query = BTreeMap::new();
    for (k, v) in url.query_pairs() {
        log::info!("🟣 [PARSE-EVENT] Query pair: {k:?} = {v:?}");
        query.insert(k.to_string(), v.to_string());
    }
    log::info!("🟣 [PARSE-EVENT] Total query pairs: {}", query.len());

    let event = DeepLinkEvent {
        url: url.to_string(),
        scheme: "nearx".into(),
        host: host.clone(),
        path: path.clone(),
        query: query.clone(),
    };

    log::info!("🟣 [PARSE-EVENT] ✅ Created DeepLinkEvent:");
    log::info!("🟣 [PARSE-EVENT]    url: {:?}", event.url);
    log::info!("🟣 [PARSE-EVENT]    host: {:?}", event.host);
    log::info!("🟣 [PARSE-EVENT]    path: {:?}", event.path);
    log::info!("🟣 [PARSE-EVENT]    query: {:?}", event.query);
    log::info!("🟣 [PARSE-EVENT] ==================== END ====================");

    Some(event)
}

fn emit_or_queue<R: Runtime>(app: &tauri::AppHandle<R>, evs: Vec<DeepLinkEvent>) {
    log::info!("🟤 [EMIT-OR-QUEUE] ==================== START ====================");
    log::info!("🟤 [EMIT-OR-QUEUE] Received {} event(s)", evs.len());

    if evs.is_empty() {
        log::info!("🟤 [EMIT-OR-QUEUE] Empty event list - returning early");
        log::info!("🟤 [EMIT-OR-QUEUE] ==================== END ====================");
        return;
    }

    for (i, ev) in evs.iter().enumerate() {
        log::info!(
            "🟤 [EMIT-OR-QUEUE] Event[{}]: url={:?}, host={:?}, path={:?}, query={:?}",
            i,
            ev.url,
            ev.host,
            ev.path,
            ev.query
        );
    }

    let is_ready = ready_get();
    log::info!("🟤 [EMIT-OR-QUEUE] Frontend ready state: {is_ready}");

    if is_ready {
        log::info!(
            "🟤 [EMIT-OR-QUEUE] ✅ Frontend ready - emitting {} deep link(s) immediately",
            evs.len()
        );
        match app.emit("deep-link", &evs) {
            Ok(_) => log::info!(
                "🟤 [EMIT-OR-QUEUE] ✅ Successfully emitted 'deep-link' event to frontend"
            ),
            Err(e) => log::error!("🟤 [EMIT-OR-QUEUE] ❌ Failed to emit event: {e}"),
        }
    } else {
        log::info!(
            "🟤 [EMIT-OR-QUEUE] ⏳ Frontend not ready - queueing {} deep link(s)",
            evs.len()
        );
        let mut q = queue().lock().unwrap();
        let prev_len = q.len();
        q.extend(evs);
        log::info!("🟤 [EMIT-OR-QUEUE] Queue size: {} → {}", prev_len, q.len());
    }

    log::info!("🟤 [EMIT-OR-QUEUE] ==================== END ====================");
}

/// Spawn the native messaging host sidecar if present in resources
#[cfg(desktop)]
fn spawn_sidecar_if_present(app: &tauri::AppHandle) {
    use std::process::Command;

    let name = if cfg!(target_os = "windows") {
        "nearx-native-host.exe"
    } else {
        "nearx-native-host"
    };

    match app.path().resolve(name, BaseDirectory::Resource) {
        Ok(path) if path.exists() => {
            log::info!("🚀 Spawning native messaging host sidecar: {path:?}");
            match Command::new(&path).spawn() {
                Ok(child) => log::info!("✅ Sidecar started with PID: {}", child.id()),
                Err(e) => log::error!("❌ Failed to spawn sidecar: {e}"),
            }
        }
        Ok(path) => log::debug!("Sidecar not found at {path:?}"),
        Err(e) => log::error!("Failed to resolve sidecar path: {e}"),
    }
}

fn handle_urls<R: Runtime>(app: &tauri::AppHandle<R>, raws: &[String]) {
    log::info!("🟢 [HANDLE-URLS] ==================== START ====================");
    log::info!("🟢 [HANDLE-URLS] Processing {} raw URL(s)", raws.len());
    for (i, r) in raws.iter().enumerate() {
        log::info!("🟢 [HANDLE-URLS] Raw[{i}] = {r:?}");
    }

    let mut out = Vec::new();
    for (i, r) in raws.iter().enumerate() {
        log::info!("🟢 [HANDLE-URLS] Processing Raw[{i}]: {r:?}");
        log::info!("🟢 [HANDLE-URLS] Calling normalize()...");
        if let Some(n) = normalize(r) {
            log::info!("🟢 [HANDLE-URLS] Normalized[{i}] = {n:?}");
            log::info!("🟢 [HANDLE-URLS] Calling parse_event()...");
            if let Some(ev) = parse_event(&n) {
                log::info!(
                    "🟢 [HANDLE-URLS] Parsed event[{}]: host={}, path={:?}, query={:?}",
                    i,
                    ev.host,
                    ev.path,
                    ev.query
                );
                // Optional: special-case nearx://nearx to open a secondary window
                if ev.host == "nearx" {
                    log::info!(
                        "[HANDLE-URLS] NEARx deep link - opening native TUI (not yet implemented)"
                    );
                    // TODO: open/focus your second window here if you have one
                } else {
                    log::info!("🟢 [HANDLE-URLS] Adding event to output queue");
                    out.push(ev);
                }
            } else {
                log::warn!("🟢 [HANDLE-URLS] parse_event() returned None for: {n:?}");
            }
        } else {
            log::warn!("🟢 [HANDLE-URLS] normalize() returned None for: {r:?}");
        }
    }

    log::info!("🟢 [HANDLE-URLS] Total events to emit/queue: {}", out.len());
    log::info!("🟢 [HANDLE-URLS] Calling emit_or_queue()...");
    emit_or_queue(app, out);

    // Also emit raw URLs for JavaScript bridge to handle
    // The JS bridge (web/deep_link.js) listens for "nearx://open" and updates location.hash
    for raw_url in raws {
        let _ = app.emit("nearx://open", raw_url);
        log::info!("🟢 [HANDLE-URLS] Emitted 'nearx://open' event with URL: {raw_url}");
    }

    log::info!("🟢 [HANDLE-URLS] ==================== END ====================");
}

/// Copy text to clipboard using Tauri clipboard plugin
#[tauri::command]
async fn copy_text(text: String, handle: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    handle
        .clipboard()
        .write_text(text)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // Single-instance FIRST so Win/Linux argv deep-links are captured.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            log::info!("🔴 [SINGLE-INSTANCE] ==================== START ====================");
            log::info!("🔴 [SINGLE-INSTANCE] Raw argv received");
            log::info!("🔴 [SINGLE-INSTANCE] argv length: {}", argv.len());
            for (i, arg) in argv.iter().enumerate() {
                log::info!("🔴 [SINGLE-INSTANCE] argv[{i}] = {arg:?}");
            }
            let urls: Vec<String> = argv.into_iter().collect();
            log::info!(
                "🔴 [SINGLE-INSTANCE] Converted to {} URL(s) for processing",
                urls.len()
            );
            log::info!("🔴 [SINGLE-INSTANCE] ==================== END ====================");
            handle_urls(app.app_handle(), &urls);
        }));
    }

    builder = builder
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_deep_link::init())
        .invoke_handler(tauri::generate_handler![
            deeplink_frontend_ready,
            copy_text
        ])
        .setup(|app| {
            log::info!("NEARx Tauri starting");

            // Auto-open DevTools in debug builds
            #[cfg(debug_assertions)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    log::info!("🔧 Auto-opening DevTools (debug mode)");
                    window.open_devtools();
                }
            }

            // Spawn native messaging host sidecar if present
            #[cfg(desktop)]
            spawn_sidecar_if_present(app.handle());

            // Dev convenience (Win/Linux). macOS requires installed app.
            #[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
            {
                #[cfg(feature = "e2e")]
                {
                    log::info!("E2E mode: skipping deep-link scheme registration");
                }
                #[cfg(not(feature = "e2e"))]
                {
                    log::info!("Registering deep link schemes (Win/Linux dev mode)");
                    app.deep_link().register_all()?;
                }
            }

            #[cfg(all(target_os = "macos", debug_assertions))]
            {
                log::warn!("macOS dev mode: Deep links require installing the built .app");
                log::warn!("   Run: cargo tauri build");
                log::warn!("   Then: open target/release/bundle/macos/NEARx.app");
            }

            // Were we launched by a deep link?
            if let Some(initial) = app.deep_link().get_current()? {
                log::info!("🟠 [GET-CURRENT] ==================== START ====================");
                log::info!("🟠 [GET-CURRENT] Raw initial URLs received from Tauri API");
                log::info!("🟠 [GET-CURRENT] Count: {}", initial.len());
                for (i, url) in initial.iter().enumerate() {
                    log::info!("🟠 [GET-CURRENT] URL[{}] = {:?}", i, url.as_str());
                }
                let urls: Vec<String> = initial.iter().map(|u| u.as_str().to_string()).collect();
                log::info!("🟠 [GET-CURRENT] Converted to Vec<String>: {urls:?}");
                log::info!("🟠 [GET-CURRENT] Calling handle_urls()...");
                log::info!("🟠 [GET-CURRENT] ==================== END ====================");
                handle_urls(app.handle(), &urls);
            } else {
                log::info!("🟠 [GET-CURRENT] No initial deep links detected");
            }

            // Deep links received while running.
            app.deep_link().on_open_url({
                let app_handle = app.handle().clone();
                move |event| {
                    log::info!("🟡 [ON-OPEN-URL] ==================== START ====================");
                    log::info!("🟡 [ON-OPEN-URL] Runtime deep link event received");
                    let event_urls = event.urls(); // Consume event once
                    log::info!("🟡 [ON-OPEN-URL] Event URLs count: {}", event_urls.len());
                    for (i, url) in event_urls.iter().enumerate() {
                        log::info!("🟡 [ON-OPEN-URL] URL[{}] = {:?}", i, url.as_str());
                    }
                    let urls: Vec<String> =
                        event_urls.iter().map(|u| u.as_str().to_string()).collect();
                    log::info!("🟡 [ON-OPEN-URL] Converted to Vec<String>: {urls:?}");
                    log::info!("🟡 [ON-OPEN-URL] Calling handle_urls()...");
                    log::info!("🟡 [ON-OPEN-URL] ==================== END ====================");
                    handle_urls(&app_handle, &urls)
                }
            });

            log::info!("NEARx Tauri setup complete");
            Ok(())
        });

    builder.run(tauri::generate_context!()).expect("run tauri");
}
