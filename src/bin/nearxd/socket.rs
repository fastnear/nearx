use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::sync::Arc;

use nearx_broker_ipc::BrokerEndpoint;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use crate::broker::{handle_request, BrokerRequest, BrokerResponse, BrokerState};

pub(crate) fn serve_connection<S>(stream: S, state: Arc<BrokerState>)
where
    S: Read + Write + Send + 'static,
{
    let mut reader = BufReader::new(stream);
    let mut line = String::new();

    loop {
        line.clear();
        let n = match reader.read_line(&mut line) {
            Ok(n) => n,
            Err(e) => {
                log::warn!("nearxd: read error: {e}");
                break;
            }
        };

        if n == 0 {
            break;
        }

        let raw = line.trim();
        if raw.is_empty() {
            continue;
        }

        let req = match serde_json::from_str::<BrokerRequest>(raw) {
            Ok(req) => req,
            Err(e) => {
                let resp = BrokerResponse::err("0".to_string(), "ERR_DECODE", e.to_string());
                let payload = match serde_json::to_string(&resp) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let _ = writeln!(reader.get_mut(), "{payload}");
                let _ = reader.get_mut().flush();
                continue;
            }
        };

        let resp = handle_request(&state, req);
        let payload = match serde_json::to_string(&resp) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("nearxd: encode response failed: {e}");
                continue;
            }
        };

        if let Err(e) = writeln!(reader.get_mut(), "{payload}") {
            log::warn!("nearxd: write error: {e}");
            break;
        }
        if let Err(e) = reader.get_mut().flush() {
            log::warn!("nearxd: flush error: {e}");
            break;
        }
    }
}

pub(crate) fn broker_endpoint() -> BrokerEndpoint {
    BrokerEndpoint::from_env()
}

pub(crate) fn prepare_socket_path(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create socket directory: {e}"))?;
    }

    match fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("remove existing socket: {e}")),
    }

    Ok(())
}

pub(crate) fn run(state: Arc<BrokerState>) -> Result<(), String> {
    let endpoint = broker_endpoint();
    if let Some(path) = endpoint.filesystem_path() {
        prepare_socket_path(path)?;
    }

    let listener = endpoint.bind().map_err(|e| format!("bind socket: {e}"))?;

    #[cfg(unix)]
    if let Some(path) = endpoint.filesystem_path() {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|e| format!("chmod socket: {e}"))?;
    }

    log::info!("nearxd listening on {}", endpoint.display());

    for stream in listener {
        match stream {
            Ok(stream) => {
                let state = state.clone();
                std::thread::spawn(move || serve_connection(stream, state));
            }
            Err(e) => {
                log::warn!("nearxd accept failed: {e}");
            }
        }
    }

    Ok(())
}
