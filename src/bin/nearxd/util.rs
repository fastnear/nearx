use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::RngCore;
use serde_json::{json, Value};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(crate) fn random_urlsafe(len_bytes: usize) -> String {
    let mut bytes = vec![0u8; len_bytes];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[allow(clippy::needless_return)]
pub(crate) fn open_url(url: &str) -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("open failed: {e}"))?;
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map_err(|e| format!("rundll32 failed: {e}"))?;
        return Ok(());
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|e| format!("xdg-open failed: {e}"))?;
        Ok(())
    }
}

pub(crate) fn route_to_json(route: &nearx::router::Route) -> Value {
    use nearx::router::{BlockRef, Network, Route, RouteV1};

    fn network_to_str(network: &Network) -> &'static str {
        match network {
            Network::Mainnet => "mainnet",
            Network::Testnet => "testnet",
            Network::Betanet => "betanet",
            Network::Localnet => "localnet",
            Network::Custom => "custom",
        }
    }

    match route {
        Route::V1(RouteV1::Home) => json!({ "version": "v1", "route": "home" }),
        Route::V1(RouteV1::Tx {
            tx_hash,
            block,
            network,
        }) => {
            let block_json = block.as_ref().map(|b| match b {
                BlockRef::Height(h) => json!({ "type": "height", "value": h }),
                BlockRef::Hash(h) => json!({ "type": "hash", "value": h }),
            });
            json!({
                "version": "v1",
                "route": "tx",
                "tx_hash": tx_hash,
                "block": block_json,
                "network": network.as_ref().map(network_to_str),
            })
        }
        Route::V1(RouteV1::Block { block, network }) => {
            let block_json = match block {
                BlockRef::Height(h) => json!({ "type": "height", "value": h }),
                BlockRef::Hash(h) => json!({ "type": "hash", "value": h }),
            };
            json!({
                "version": "v1",
                "route": "block",
                "block": block_json,
                "network": network.as_ref().map(network_to_str),
            })
        }
        Route::V1(RouteV1::Account {
            account_id,
            network,
        }) => json!({
            "version": "v1",
            "route": "account",
            "account_id": account_id,
            "network": network.as_ref().map(network_to_str),
        }),
        Route::V1(RouteV1::Contract {
            account_id,
            method,
            network,
        }) => json!({
            "version": "v1",
            "route": "contract",
            "account_id": account_id,
            "method": method,
            "network": network.as_ref().map(network_to_str),
        }),
        Route::V1(RouteV1::AccessKey {
            account_id,
            public_key,
            network,
        }) => json!({
            "version": "v1",
            "route": "access-key",
            "account_id": account_id,
            "public_key": public_key,
            "network": network.as_ref().map(network_to_str),
        }),
        Route::V1(RouteV1::Staking {
            account_id,
            network,
        }) => json!({
            "version": "v1",
            "route": "staking",
            "account_id": account_id,
            "network": network.as_ref().map(network_to_str),
        }),
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn run_swift_json(script: &str, args: &[&str]) -> Result<Value, String> {
    use std::io::ErrorKind;
    use std::process::Command;

    let output = Command::new("swift")
        .arg("-e")
        .arg(script)
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                "swift not found; install Xcode Command Line Tools to enable biometric prompts"
                    .to_string()
            } else {
                format!("swift invocation failed: {e}")
            }
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(format!("swift exited with status {}", output.status));
        }
        return Err(stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return Err("swift returned empty response".to_string());
    }

    serde_json::from_str::<Value>(&stdout).map_err(|e| {
        format!(
            "decode swift JSON response failed: {e}; response={}",
            stdout
        )
    })
}
