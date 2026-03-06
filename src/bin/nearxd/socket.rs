use std::env;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

use crate::broker::{handle_request, BrokerRequest, BrokerResponse, BrokerState};

pub(crate) trait CloneableStream: Read + Write + Send + 'static {
    fn clone_stream(&self) -> std::io::Result<Self>
    where
        Self: Sized;
}

#[cfg(unix)]
impl CloneableStream for UnixStream {
    fn clone_stream(&self) -> std::io::Result<Self> {
        self.try_clone()
    }
}

pub(crate) fn serve_connection<S>(stream: S, state: Arc<BrokerState>)
where
    S: CloneableStream,
{
    let peer_reader = match stream.clone_stream() {
        Ok(s) => s,
        Err(e) => {
            log::warn!("nearxd: clone stream failed: {e}");
            return;
        }
    };

    let mut reader = BufReader::new(peer_reader);
    let mut writer = BufWriter::new(stream);
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
                let _ = writeln!(writer, "{payload}");
                let _ = writer.flush();
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

        if let Err(e) = writeln!(writer, "{payload}") {
            log::warn!("nearxd: write error: {e}");
            break;
        }
        if let Err(e) = writer.flush() {
            log::warn!("nearxd: flush error: {e}");
            break;
        }
    }
}

#[cfg(unix)]
pub(crate) fn socket_path() -> PathBuf {
    if let Ok(path) = env::var("NEARXD_SOCKET_PATH") {
        let p = PathBuf::from(path.trim());
        if !p.as_os_str().is_empty() {
            return p;
        }
    }

    env::temp_dir().join("nearxd.sock")
}

#[cfg(unix)]
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

#[cfg(unix)]
pub(crate) fn run_unix(state: Arc<BrokerState>) -> Result<(), String> {
    let path = socket_path();
    prepare_socket_path(&path)?;

    let listener = UnixListener::bind(&path).map_err(|e| format!("bind socket: {e}"))?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .map_err(|e| format!("chmod socket: {e}"))?;

    log::info!("nearxd listening on {}", path.display());

    for stream in listener.incoming() {
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
