use anyhow::{Context, Result};
use nearx_broker_ipc::BrokerEndpoint;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::process::Command;

const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum InMsg {
    Hello {
        #[allow(dead_code)]
        requested_version: Option<u16>,
    },
    Ping {
        id: String,
    },
    OpenDeepLink {
        url: String,
    },
    OpenSession {
        id: String,
        read_only: bool,
    },
    CreateSignIntent {
        account_id: String,
        payload: Value,
        origin: Option<String>,
        expires_in_ms: Option<u64>,
        require_user_presence: Option<bool>,
        user_presence_reason: Option<String>,
    },
    ApproveSignIntent {
        intent_id: String,
        challenge: String,
    },
    ConsumeSignIntent {
        intent_id: String,
        challenge: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutMsg<'a> {
    Hello { version: u16 },
    Pong { id: &'a str },
    Ok { op: &'a str },
    Data { op: &'a str, data: Value },
    Err { op: &'a str, message: String },
}

fn read_msg(stdin: &mut impl Read) -> Result<Option<serde_json::Value>> {
    let mut len_buf = [0u8; 4];
    if stdin.read_exact(&mut len_buf).is_err() {
        return Ok(None);
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    let mut buf = vec![0u8; len];
    stdin.read_exact(&mut buf).context("read payload")?;
    Ok(Some(serde_json::from_slice(&buf).context("json parse")?))
}

fn write_msg(stdout: &mut impl Write, v: &serde_json::Value) -> Result<()> {
    let bytes = serde_json::to_vec(v)?;
    stdout.write_all(&(bytes.len() as u32).to_le_bytes())?;
    stdout.write_all(&bytes)?;
    stdout.flush()?;
    Ok(())
}

fn open_url(url: &str) -> Result<()> {
    if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()?;
    } else if cfg!(target_os = "windows") {
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()?;
    } else {
        Command::new("xdg-open").arg(url).spawn()?;
    }
    Ok(())
}

fn nearxd_endpoint() -> BrokerEndpoint {
    BrokerEndpoint::from_env()
}

fn nearxd_request(method: &str, params: Value) -> Result<Value> {
    let endpoint = nearxd_endpoint();
    let mut stream = endpoint
        .connect()
        .with_context(|| format!("connect nearxd socket at {}", endpoint.display()))?;

    let req = json!({
        "id": "native-host",
        "method": method,
        "params": params
    });

    let payload = serde_json::to_string(&req).context("encode nearxd request")?;
    stream
        .write_all(payload.as_bytes())
        .context("write nearxd request")?;
    stream
        .write_all(b"\n")
        .context("write nearxd request delimiter")?;
    stream.flush().context("flush nearxd request")?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .context("read nearxd response")?;

    if line.trim().is_empty() {
        anyhow::bail!("nearxd returned empty response");
    }

    let resp: serde_json::Value =
        serde_json::from_str(line.trim()).context("decode nearxd response")?;
    if resp.get("ok").and_then(|v| v.as_bool()) == Some(true) {
        return Ok(resp.get("result").cloned().unwrap_or_else(|| json!({})));
    }

    let err = resp
        .pointer("/error/message")
        .and_then(|v| v.as_str())
        .unwrap_or("nearxd rejected request");

    anyhow::bail!(err.to_string())
}

fn open_url_via_nearxd(url: &str) -> Result<()> {
    nearxd_request("open_deep_link", json!({ "url": url })).map(|_| ())
}

fn open_url_broker_first(url: &str) -> Result<()> {
    match open_url_via_nearxd(url) {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("connect nearxd socket") || msg.contains("No such file or directory") {
                eprintln!(
                    "[nearx-native-host] nearxd unavailable ({e}); falling back to direct OS open"
                );
                open_url(url)
            } else {
                anyhow::bail!(msg);
            }
        }
    }
}

fn main() -> Result<()> {
    let mut stdin = io::stdin().lock();
    let mut stdout = io::stdout().lock();

    // Optional: send Hello immediately so the extension learns our version.
    write_msg(
        &mut stdout,
        &serde_json::to_value(OutMsg::Hello {
            version: PROTOCOL_VERSION,
        })?,
    )?;

    loop {
        let Some(v) = read_msg(&mut stdin)? else {
            break;
        };
        let msg: Result<InMsg> = serde_json::from_value(v.clone()).context("invalid message");

        match msg {
            Ok(InMsg::Hello {
                requested_version: _,
            }) => {
                write_msg(
                    &mut stdout,
                    &serde_json::to_value(OutMsg::Hello {
                        version: PROTOCOL_VERSION,
                    })?,
                )?;
            }
            Ok(InMsg::Ping { id }) => {
                write_msg(
                    &mut stdout,
                    &serde_json::to_value(OutMsg::Pong { id: &id })?,
                )?;
            }
            Ok(InMsg::OpenDeepLink { url }) => {
                let op = "open_deep_link";
                match open_url_broker_first(&url) {
                    Ok(_) => write_msg(&mut stdout, &serde_json::to_value(OutMsg::Ok { op })?)?,
                    Err(e) => write_msg(
                        &mut stdout,
                        &serde_json::to_value(OutMsg::Err {
                            op,
                            message: e.to_string(),
                        })?,
                    )?,
                }
            }
            Ok(InMsg::OpenSession { id, read_only }) => {
                let op = "open_session";
                let url = format!(
                    "near://open/session/{}?readOnly={}",
                    id,
                    if read_only { 1 } else { 0 }
                );
                match open_url(&url) {
                    Ok(_) => write_msg(&mut stdout, &serde_json::to_value(OutMsg::Ok { op })?)?,
                    Err(e) => write_msg(
                        &mut stdout,
                        &serde_json::to_value(OutMsg::Err {
                            op,
                            message: e.to_string(),
                        })?,
                    )?,
                }
            }
            Ok(InMsg::CreateSignIntent {
                account_id,
                payload,
                origin,
                expires_in_ms,
                require_user_presence,
                user_presence_reason,
            }) => {
                let op = "create_sign_intent";
                #[cfg(unix)]
                {
                    let mut params = json!({
                        "account_id": account_id,
                        "payload": payload,
                    });
                    if let Some(o) = origin {
                        params["origin"] = json!(o);
                    }
                    if let Some(ttl) = expires_in_ms {
                        params["expires_in_ms"] = json!(ttl);
                    }
                    if let Some(require) = require_user_presence {
                        params["require_user_presence"] = json!(require);
                    }
                    if let Some(reason) = user_presence_reason {
                        params["user_presence_reason"] = json!(reason);
                    }

                    match nearxd_request(op, params) {
                        Ok(data) => {
                            write_msg(
                                &mut stdout,
                                &serde_json::to_value(OutMsg::Data { op, data })?,
                            )?;
                        }
                        Err(e) => {
                            write_msg(
                                &mut stdout,
                                &serde_json::to_value(OutMsg::Err {
                                    op,
                                    message: e.to_string(),
                                })?,
                            )?;
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    write_msg(
                        &mut stdout,
                        &serde_json::to_value(OutMsg::Err {
                            op,
                            message: "sign intent forwarding is unavailable on this platform"
                                .to_string(),
                        })?,
                    )?;
                }
            }
            Ok(InMsg::ApproveSignIntent {
                intent_id,
                challenge,
            }) => {
                let op = "approve_sign_intent";
                #[cfg(unix)]
                {
                    match nearxd_request(
                        op,
                        json!({
                            "intent_id": intent_id,
                            "challenge": challenge
                        }),
                    ) {
                        Ok(data) => {
                            write_msg(
                                &mut stdout,
                                &serde_json::to_value(OutMsg::Data { op, data })?,
                            )?;
                        }
                        Err(e) => {
                            write_msg(
                                &mut stdout,
                                &serde_json::to_value(OutMsg::Err {
                                    op,
                                    message: e.to_string(),
                                })?,
                            )?;
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    write_msg(
                        &mut stdout,
                        &serde_json::to_value(OutMsg::Err {
                            op,
                            message: "sign intent forwarding is unavailable on this platform"
                                .to_string(),
                        })?,
                    )?;
                }
            }
            Ok(InMsg::ConsumeSignIntent {
                intent_id,
                challenge,
            }) => {
                let op = "consume_sign_intent";
                #[cfg(unix)]
                {
                    match nearxd_request(
                        op,
                        json!({
                            "intent_id": intent_id,
                            "challenge": challenge
                        }),
                    ) {
                        Ok(data) => {
                            write_msg(
                                &mut stdout,
                                &serde_json::to_value(OutMsg::Data { op, data })?,
                            )?;
                        }
                        Err(e) => {
                            write_msg(
                                &mut stdout,
                                &serde_json::to_value(OutMsg::Err {
                                    op,
                                    message: e.to_string(),
                                })?,
                            )?;
                        }
                    }
                }
                #[cfg(not(unix))]
                {
                    write_msg(
                        &mut stdout,
                        &serde_json::to_value(OutMsg::Err {
                            op,
                            message: "sign intent forwarding is unavailable on this platform"
                                .to_string(),
                        })?,
                    )?;
                }
            }
            Err(e) => {
                write_msg(
                    &mut stdout,
                    &serde_json::to_value(OutMsg::Err {
                        op: "decode",
                        message: e.to_string(),
                    })?,
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::Mutex;
    use std::thread;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_socket_path<T>(f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap();
        let old = std::env::var("NEARXD_SOCKET_PATH").ok();
        let unique = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let path = std::env::temp_dir().join(format!("nearx-native-host-test-{unique}.sock",));

        let _ = std::fs::remove_file(&path);
        std::env::set_var("NEARXD_SOCKET_PATH", &path);
        let out = f();
        if let Some(prev) = old {
            std::env::set_var("NEARXD_SOCKET_PATH", prev);
        } else {
            std::env::remove_var("NEARXD_SOCKET_PATH");
        }
        let _ = std::fs::remove_file(path);
        out
    }

    #[test]
    fn nearxd_open_deep_link_protocol_roundtrip() {
        with_temp_socket_path(|| {
            let socket = nearxd_socket_path();
            let listener = match UnixListener::bind(&socket) {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    eprintln!("skipping test: cannot bind unix socket in this environment");
                    return;
                }
                Err(e) => panic!("failed to bind test socket: {e}"),
            };

            let handle = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();

                let req: Value = serde_json::from_str(line.trim()).unwrap();
                assert_eq!(
                    req.get("method").and_then(Value::as_str),
                    Some("open_deep_link")
                );
                assert_eq!(
                    req.pointer("/params/url").and_then(Value::as_str),
                    Some("nearx://v1/home")
                );

                let resp = serde_json::json!({
                    "id": "native-host",
                    "ok": true,
                    "result": { "opened": true }
                });
                writeln!(stream, "{resp}").unwrap();
                stream.flush().unwrap();
            });

            let result = open_url_via_nearxd("nearx://v1/home");
            handle.join().unwrap();
            assert!(result.is_ok());
        });
    }

    #[test]
    fn nearxd_error_is_propagated() {
        with_temp_socket_path(|| {
            let socket = nearxd_socket_path();
            let listener = match UnixListener::bind(&socket) {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                    eprintln!("skipping test: cannot bind unix socket in this environment");
                    return;
                }
                Err(e) => panic!("failed to bind test socket: {e}"),
            };

            let handle = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();

                let req: Value = serde_json::from_str(line.trim()).unwrap();
                assert_eq!(
                    req.get("method").and_then(Value::as_str),
                    Some("open_deep_link")
                );

                let resp = serde_json::json!({
                    "id": "native-host",
                    "ok": false,
                    "error": { "code": "ERR_ROUTE", "message": "invalid route" }
                });
                writeln!(stream, "{resp}").unwrap();
                stream.flush().unwrap();
            });

            let result = open_url_via_nearxd("nearx://bad");
            handle.join().unwrap();
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("invalid route"));
        });
    }
}
