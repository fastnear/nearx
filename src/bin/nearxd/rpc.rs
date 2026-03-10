use serde::Deserialize;
use serde_json::{json, Value};

use crate::config::runtime_near_node_url;

#[derive(Debug, Deserialize)]
pub(crate) struct RpcAccessKeyListItem {
    pub public_key: String,
    #[serde(default)]
    pub access_key: Value,
}

#[derive(Debug, Deserialize)]
struct RpcAccessKeyListResult {
    keys: Vec<RpcAccessKeyListItem>,
}

pub(crate) fn fetch_onchain_access_keys(
    account_id: &str,
) -> Result<Vec<RpcAccessKeyListItem>, String> {
    let rpc_url = runtime_near_node_url();
    let body = json!({
        "jsonrpc": "2.0",
        "id": "nearxd-list-keys",
        "method": "query",
        "params": {
            "request_type": "view_access_key_list",
            "finality": "final",
            "account_id": account_id,
        }
    });

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("create async runtime: {e}"))?;

    runtime.block_on(async move {
        let client = reqwest::Client::builder()
            .default_headers({
                let mut h = reqwest::header::HeaderMap::new();
                h.insert(
                    "X-Nearx-Client",
                    concat!("nearxd/", env!("CARGO_PKG_VERSION"))
                        .parse()
                        .unwrap(),
                );
                h
            })
            .build()
            .expect("reqwest client");
        let resp = client
            .post(rpc_url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("rpc request failed: {e}"))?;
        let value = resp
            .json::<Value>()
            .await
            .map_err(|e| format!("rpc decode failed: {e}"))?;
        if let Some(err) = value.get("error") {
            return Err(format!("rpc returned error: {err}"));
        }
        let result = value
            .get("result")
            .cloned()
            .ok_or_else(|| "rpc response missing result".to_string())?;
        let parsed: RpcAccessKeyListResult =
            serde_json::from_value(result).map_err(|e| format!("parse access key list: {e}"))?;
        Ok(parsed.keys)
    })
}

pub(crate) fn access_key_permission_to_summary(access_key: &Value) -> Value {
    let permission = access_key
        .get("permission")
        .cloned()
        .unwrap_or_else(|| access_key.clone());

    if permission == Value::String("FullAccess".to_string())
        || permission.get("FullAccess").is_some()
    {
        return json!({ "kind": "full_access" });
    }

    let function_call = permission
        .get("FunctionCall")
        .or_else(|| permission.get("function_call"));
    if let Some(fc) = function_call {
        return json!({
            "kind": "function_call",
            "receiver_id": fc.get("receiver_id").and_then(Value::as_str).unwrap_or_default(),
            "method_names": fc.get("method_names").and_then(Value::as_array).cloned().unwrap_or_default(),
            "allowance": fc.get("allowance").and_then(Value::as_str),
        });
    }

    json!({ "kind": "unknown" })
}

pub(crate) fn is_full_access_permission(permission: &Value) -> bool {
    permission
        .get("kind")
        .and_then(Value::as_str)
        .map(|kind| kind == "full_access")
        .unwrap_or(false)
}
