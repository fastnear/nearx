use base64::{engine::general_purpose::STANDARD, Engine as _};
use near_crypto::{PublicKey, SecretKey};
use near_primitives::action::Action;
use near_primitives::hash::CryptoHash;
use near_primitives::transaction::{SignedTransaction, Transaction, TransactionV0};
use near_primitives::types::AccountId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::sync::{Arc, Mutex};

use crate::config::{
    expand_tilde_path, near_credentials_dir, runtime_fastnear_api_url, runtime_near_node_url,
};
use crate::credentials::{
    collect_legacy_credentials, nearxd_credential_account_legacy, nearxd_keychain_has_credential,
    normalize_source, read_near_cli_secure_credential, read_near_credential_keychain,
    KEYCHAIN_NEAR_CREDENTIAL_SERVICE, SOURCE_HARDWARE_WALLET, SOURCE_LEGACY_FILE,
    SOURCE_NEARXD_KEYCHAIN, SOURCE_NEAR_CLI_SECURE,
};
use crate::hardware_wallet::{
    connect_hardware_wallet_result, hardware_wallet_sign_transaction,
    resolve_hardware_wallet_record,
};
use crate::keychain::keychain_has_generic;
use crate::settings::HardwareWalletIndexRecord;
use crate::settings::{
    add_staking_watchlist_account_result, list_staking_watchlist_result, load_signing_settings,
    persist_signing_settings, remove_staking_watchlist_account_result,
};
use crate::signing::{
    import_near_signing_keys_result, list_near_signing_accounts_result,
    list_near_signing_keys_result, resolve_signing_credential,
};
use crate::token::{build_token_store, TokenStore};
use crate::user_presence::{
    probe_user_presence, request_user_presence, DEFAULT_USER_PRESENCE_REASON,
};
use crate::util::{now_ms, open_url, random_urlsafe, route_to_json};

pub(crate) const BROKER_VERSION: u8 = 1;
pub(crate) const DEFAULT_INTENT_TTL_MS: u64 = 2 * 60 * 1000; // 2 minutes
pub(crate) const MAX_INTENT_TTL_MS: u64 = 10 * 60 * 1000; // 10 minutes

#[derive(Debug)]
pub(crate) struct BrokerState {
    pub session_token: Mutex<Option<String>>,
    pub token_store: Arc<dyn TokenStore>,
    pub sign_intents: Mutex<std::collections::HashMap<String, SignIntent>>,
}

impl BrokerState {
    pub fn new() -> Self {
        Self {
            session_token: Mutex::new(None),
            token_store: build_token_store(),
            sign_intents: Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl Default for BrokerState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SignIntent {
    pub account_id: String,
    pub payload: Value,
    pub origin: Option<String>,
    pub challenge: String,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
    pub approved: bool,
    pub require_user_presence: bool,
    pub user_presence_reason: Option<String>,
    pub user_presence_verified: bool,
    pub user_presence_modality: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BrokerRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct BrokerResponse {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BrokerError>,
}

#[derive(Debug, Serialize)]
pub(crate) struct BrokerError {
    pub code: &'static str,
    pub message: String,
}

impl BrokerResponse {
    pub fn ok(id: String, result: Value) -> Self {
        Self {
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: String, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            result: None,
            error: Some(BrokerError {
                code,
                message: message.into(),
            }),
        }
    }
}

pub(crate) fn id_or_default(id: Option<String>) -> String {
    id.unwrap_or_else(|| "0".to_string())
}

pub(crate) fn parse_bool(params: &Value, key: &str, default: bool) -> bool {
    params.get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub(crate) fn parse_string<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str).map(str::trim)
}

pub(crate) fn parse_u64(params: &Value, key: &str, default: u64) -> u64 {
    params.get(key).and_then(Value::as_u64).unwrap_or(default)
}

pub(crate) fn parse_optional_bool(params: &Value, key: &str) -> Option<bool> {
    params.get(key).and_then(Value::as_bool)
}

pub(crate) fn parse_near_actions(actions_json: &[Value]) -> Result<Vec<Action>, String> {
    use near_primitives::action::{FunctionCallAction, TransferAction};

    let mut actions = Vec::with_capacity(actions_json.len());
    for (i, a) in actions_json.iter().enumerate() {
        let action_type = a
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("action[{i}]: missing 'type' string"))?;
        match action_type {
            "Transfer" => {
                let deposit_str = a
                    .get("deposit")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("action[{i}]: Transfer requires 'deposit' string"))?;
                let deposit: u128 = deposit_str
                    .parse()
                    .map_err(|e| format!("action[{i}]: invalid deposit: {e}"))?;
                actions.push(Action::Transfer(TransferAction { deposit }));
            }
            "FunctionCall" => {
                let method_name = a
                    .get("method_name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        format!("action[{i}]: FunctionCall requires 'method_name' string")
                    })?
                    .to_string();
                let args_b64 = a.get("args").and_then(Value::as_str).unwrap_or("");
                let args = STANDARD
                    .decode(args_b64)
                    .map_err(|e| format!("action[{i}]: invalid base64 args: {e}"))?;
                let gas = a
                    .get("gas")
                    .and_then(Value::as_u64)
                    .unwrap_or(30_000_000_000_000); // 30 TGas default
                let deposit_str = a.get("deposit").and_then(Value::as_str).unwrap_or("0");
                let deposit: u128 = deposit_str
                    .parse()
                    .map_err(|e| format!("action[{i}]: invalid deposit: {e}"))?;
                actions.push(Action::FunctionCall(Box::new(FunctionCallAction {
                    method_name,
                    args,
                    gas,
                    deposit,
                })));
            }
            other => {
                return Err(format!(
                    "action[{i}]: unsupported action type '{other}' (supported: Transfer, FunctionCall)"
                ));
            }
        }
    }
    Ok(actions)
}

pub(crate) fn resolve_network_param(params: &Value, default: &str) -> Result<String, String> {
    let network = parse_string(params, "network")
        .unwrap_or(default)
        .trim()
        .to_ascii_lowercase();
    if network.is_empty() {
        return Err("network cannot be empty".to_string());
    }
    Ok(network)
}

pub(crate) fn resolve_fastnear_token(state: &BrokerState) -> (Option<String>, &'static str) {
    if let Ok(guard) = state.session_token.lock() {
        if let Some(tok) = guard.as_ref().filter(|t| !t.trim().is_empty()) {
            return (Some(tok.clone()), "session");
        }
    }

    if let Some(tok) = state.token_store.read_token() {
        return (Some(tok), state.token_store.backend_name());
    }

    if let Ok(tok) = env::var("FASTNEAR_API_KEY") {
        if !tok.trim().is_empty() {
            return (Some(tok), "env_api_key");
        }
    }

    if let Ok(tok) = env::var("FASTNEAR_AUTH_TOKEN") {
        if !tok.trim().is_empty() {
            return (Some(tok), "env_auth_token");
        }
    }

    (None, "none")
}

pub(crate) fn cleanup_expired_intents(state: &BrokerState) {
    let now = now_ms();
    if let Ok(mut intents) = state.sign_intents.lock() {
        intents.retain(|_, v| v.expires_at_ms > now);
    }
}

pub(crate) fn handle_request(state: &Arc<BrokerState>, req: BrokerRequest) -> BrokerResponse {
    let id = id_or_default(req.id);

    match req.method.as_str() {
        "ping" => BrokerResponse::ok(
            id,
            json!({
                "name": "nearxd",
                "version": BROKER_VERSION
            }),
        ),

        "get_runtime_config" => {
            let (token, source) = resolve_fastnear_token(state);
            let include_token = parse_bool(&req.params, "include_token", false);
            let has_token = token.is_some();
            let token_legacy = if include_token { token.clone() } else { None };
            let token_api = if include_token { token } else { None };
            BrokerResponse::ok(
                id,
                json!({
                    "near_node_url": runtime_near_node_url(),
                    "fastnear_api_url": runtime_fastnear_api_url(),
                    "has_fastnear_auth_token": has_token,
                    "has_fastnear_api_key": has_token,
                    "fastnear_auth_token_source": source,
                    "fastnear_api_key_source": source,
                    "fastnear_auth_token": token_legacy,
                    "fastnear_api_key": token_api,
                    "token_backend": state.token_store.backend_name(),
                }),
            )
        }

        "resolve_fastnear_auth_token"
        | "get_fastnear_auth_token"
        | "resolve_fastnear_api_key"
        | "get_fastnear_api_key" => {
            let (token, source) = resolve_fastnear_token(state);
            let token_legacy = token.clone();
            BrokerResponse::ok(
                id,
                json!({
                    "token": token_legacy,
                    "api_key": token,
                    "source": source
                }),
            )
        }

        "set_fastnear_auth_token" | "set_fastnear_api_key" => {
            let Some(token) = parse_string(&req.params, "token").map(str::to_string) else {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "missing required string param: token",
                );
            };

            if token.is_empty() {
                return BrokerResponse::err(id, "ERR_PARAMS", "token cannot be empty");
            }

            let persist = parse_bool(&req.params, "persist", true);

            if let Ok(mut guard) = state.session_token.lock() {
                *guard = Some(token.clone());
            }

            if persist {
                if let Err(e) = state.token_store.persist_token(&token) {
                    return BrokerResponse::err(id, "ERR_PERSIST", e);
                }
            }

            BrokerResponse::ok(
                id,
                json!({
                    "stored": true,
                    "persisted": persist,
                    "backend": state.token_store.backend_name(),
                    "key_name": "fastnear_api_key",
                }),
            )
        }

        "clear_fastnear_auth_token" | "clear_fastnear_api_key" => {
            if let Ok(mut guard) = state.session_token.lock() {
                *guard = None;
            }

            if let Err(e) = state.token_store.clear_token() {
                return BrokerResponse::err(id, "ERR_PERSIST", e);
            }

            BrokerResponse::ok(
                id,
                json!({
                    "cleared": true,
                    "backend": state.token_store.backend_name(),
                    "key_name": "fastnear_api_key",
                }),
            )
        }

        "open_deep_link" => {
            let Some(url_raw) = parse_string(&req.params, "url") else {
                return BrokerResponse::err(id, "ERR_PARAMS", "missing required string param: url");
            };

            let parsed = match nearx::router::parse_deep_link(url_raw) {
                Ok(p) => p,
                Err(e) => return BrokerResponse::err(id, e.code(), e.to_string()),
            };
            let url = parsed.canonical_uri;

            if let Err(e) = open_url(&url) {
                return BrokerResponse::err(id, "ERR_OPEN", e);
            }

            BrokerResponse::ok(
                id,
                json!({
                    "opened": true,
                    "url": url
                }),
            )
        }

        "parse_deep_link" => {
            let Some(url_raw) = parse_string(&req.params, "url") else {
                return BrokerResponse::err(id, "ERR_PARAMS", "missing required string param: url");
            };

            let parsed = match nearx::router::parse_deep_link(url_raw) {
                Ok(p) => p,
                Err(e) => return BrokerResponse::err(id, e.code(), e.to_string()),
            };

            BrokerResponse::ok(
                id,
                json!({
                    "canonical_url": parsed.canonical_uri,
                    "route": route_to_json(&parsed.route),
                }),
            )
        }

        "probe_user_presence" => {
            let allow_fallback = parse_bool(&req.params, "allow_fallback", true);
            BrokerResponse::ok(id, probe_user_presence(allow_fallback))
        }

        "request_user_presence" => {
            let reason =
                parse_string(&req.params, "reason").unwrap_or(DEFAULT_USER_PRESENCE_REASON);
            let allow_fallback = parse_bool(&req.params, "allow_fallback", true);
            match request_user_presence(reason, allow_fallback) {
                Ok(v) => BrokerResponse::ok(id, v),
                Err(e) => BrokerResponse::err(id, "ERR_AUTH", e),
            }
        }

        "get_signing_settings" => {
            let (settings, source) = load_signing_settings();
            BrokerResponse::ok(
                id,
                json!({
                    "settings": settings,
                    "source": source,
                }),
            )
        }

        "set_signing_settings" => {
            let Some(settings) = req.params.get("settings").cloned() else {
                return BrokerResponse::err(id, "ERR_PARAMS", "missing required param: settings");
            };
            let prefer_keychain = parse_bool(&req.params, "prefer_keychain", true);
            match persist_signing_settings(&settings, prefer_keychain) {
                Ok(source) => BrokerResponse::ok(
                    id,
                    json!({
                        "stored": true,
                        "source": source,
                    }),
                ),
                Err(e) => BrokerResponse::err(id, "ERR_PERSIST", e),
            }
        }

        "list_staking_watchlist" => match list_staking_watchlist_result(&req.params) {
            Ok(v) => BrokerResponse::ok(id, v),
            Err((code, msg)) => BrokerResponse::err(id, code, msg),
        },

        "add_staking_watchlist_account" => {
            match add_staking_watchlist_account_result(&req.params) {
                Ok(v) => BrokerResponse::ok(id, v),
                Err((code, msg)) => BrokerResponse::err(id, code, msg),
            }
        }

        "remove_staking_watchlist_account" => {
            match remove_staking_watchlist_account_result(&req.params) {
                Ok(v) => BrokerResponse::ok(id, v),
                Err((code, msg)) => BrokerResponse::err(id, code, msg),
            }
        }

        "connect_hardware_wallet" => match connect_hardware_wallet_result(&req.params) {
            Ok(v) => BrokerResponse::ok(id, v),
            Err((code, msg)) => BrokerResponse::err(id, code, msg),
        },

        "list_near_signing_accounts" => match list_near_signing_accounts_result(&req.params) {
            Ok(v) => BrokerResponse::ok(id, v),
            Err((code, msg)) => BrokerResponse::err(id, code, msg),
        },

        "list_near_signing_keys" => match list_near_signing_keys_result(&req.params) {
            Ok(v) => BrokerResponse::ok(id, v),
            Err((code, msg)) => BrokerResponse::err(id, code, msg),
        },

        "import_near_signing_keys" => match import_near_signing_keys_result(&req.params, None) {
            Ok(v) => BrokerResponse::ok(id, v),
            Err((code, msg)) => BrokerResponse::err(id, code, msg),
        },

        // Compatibility wrapper for existing frontends.
        "list_near_credentials" => {
            let network = match resolve_network_param(&req.params, "mainnet") {
                Ok(v) => v,
                Err(e) => return BrokerResponse::err(id, "ERR_PARAMS", e),
            };
            let credentials_dir = if let Some(raw) = parse_string(&req.params, "credentials_dir") {
                expand_tilde_path(raw)
            } else {
                match near_credentials_dir(&network) {
                    Some(p) => p,
                    None => {
                        return BrokerResponse::ok(
                            id,
                            json!({
                                "network": network,
                                "credentials_dir": "~/.near-credentials/".to_string() + &network,
                                "accounts": [],
                            }),
                        );
                    }
                }
            };
            let dir_display = credentials_dir.display().to_string();
            if !credentials_dir.exists() {
                return BrokerResponse::ok(
                    id,
                    json!({
                        "network": network,
                        "credentials_dir": dir_display,
                        "accounts": [],
                    }),
                );
            }
            let legacy = collect_legacy_credentials(&credentials_dir).unwrap_or_default();
            let mut accounts = Vec::new();
            for entry in legacy {
                let account_id = entry.credential.account_id.clone();
                let public_key = entry.credential.public_key.clone();
                let in_keychain =
                    nearxd_keychain_has_credential(&network, &account_id, &public_key)
                        || keychain_has_generic(
                            KEYCHAIN_NEAR_CREDENTIAL_SERVICE,
                            &nearxd_credential_account_legacy(&network, &account_id),
                        );
                accounts.push(json!({
                    "account_id": account_id,
                    "public_key": public_key,
                    "in_keychain": in_keychain,
                }));
            }
            BrokerResponse::ok(
                id,
                json!({
                    "network": network,
                    "credentials_dir": dir_display,
                    "accounts": accounts,
                }),
            )
        }

        // Compatibility wrapper for existing frontends.
        "import_near_credentials" => {
            let result = import_near_signing_keys_result(
                &req.params,
                Some(vec![SOURCE_LEGACY_FILE.to_string()]),
            );
            match result {
                Ok(v) => BrokerResponse::ok(id, v),
                Err((code, msg)) => BrokerResponse::err(id, code, msg),
            }
        }

        "get_near_credential" => {
            let network = match resolve_network_param(&req.params, "testnet") {
                Ok(v) => v,
                Err(e) => return BrokerResponse::err(id, "ERR_PARAMS", e),
            };
            let Some(account_id) = parse_string(&req.params, "account_id") else {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "missing required string param: account_id",
                );
            };
            let signer_public_key = parse_string(&req.params, "public_key");
            let reason = parse_string(&req.params, "reason")
                .unwrap_or("NEARx needs your approval to access this credential.");
            let source = parse_string(&req.params, "credential_source")
                .and_then(normalize_source)
                .unwrap_or(SOURCE_NEARXD_KEYCHAIN);

            let result = match source {
                SOURCE_NEAR_CLI_SECURE => {
                    let Some(public_key) = signer_public_key else {
                        return BrokerResponse::err(
                            id,
                            "ERR_PARAMS",
                            "public_key is required when credential_source=near_cli_secure",
                        );
                    };
                    read_near_cli_secure_credential(&network, account_id, public_key)
                }
                _ => read_near_credential_keychain(&network, account_id, signer_public_key, reason),
            };

            match result {
                Ok(payload) => BrokerResponse::ok(
                    id,
                    json!({
                        "network": network,
                        "account_id": account_id,
                        "public_key": signer_public_key,
                        "credential_source": source,
                        "credential": payload,
                    }),
                ),
                Err(e) => {
                    let code = if !cfg!(target_os = "macos") {
                        "ERR_UNAVAILABLE"
                    } else {
                        "ERR_AUTH"
                    };
                    BrokerResponse::err(id, code, e)
                }
            }
        }

        "create_sign_intent" => {
            cleanup_expired_intents(state);

            let Some(account_id) = parse_string(&req.params, "account_id").map(str::to_string)
            else {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "missing required string param: account_id",
                );
            };
            let Some(payload) = req.params.get("payload").cloned() else {
                return BrokerResponse::err(id, "ERR_PARAMS", "missing required param: payload");
            };

            let ttl_ms = req
                .params
                .get("expires_in_ms")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_INTENT_TTL_MS)
                .clamp(1_000, MAX_INTENT_TTL_MS);
            let created_at_ms = now_ms();
            let expires_at_ms = created_at_ms + ttl_ms;

            let intent_id = random_urlsafe(16);
            let challenge = random_urlsafe(24);
            let origin = parse_string(&req.params, "origin").map(str::to_string);
            let require_user_presence = parse_bool(&req.params, "require_user_presence", false);
            let user_presence_reason =
                parse_string(&req.params, "user_presence_reason").map(str::to_string);

            let intent = SignIntent {
                account_id: account_id.clone(),
                payload,
                origin: origin.clone(),
                challenge: challenge.clone(),
                created_at_ms,
                expires_at_ms,
                approved: false,
                require_user_presence,
                user_presence_reason: user_presence_reason.clone(),
                user_presence_verified: false,
                user_presence_modality: None,
            };

            if let Ok(mut intents) = state.sign_intents.lock() {
                intents.insert(intent_id.clone(), intent);
            } else {
                return BrokerResponse::err(id, "ERR_STATE", "failed to acquire sign intent lock");
            }

            BrokerResponse::ok(
                id,
                json!({
                    "intent_id": intent_id,
                    "challenge": challenge,
                    "account_id": account_id,
                    "origin": origin,
                    "created_at_ms": created_at_ms,
                    "expires_at_ms": expires_at_ms,
                    "require_user_presence": require_user_presence,
                    "user_presence_reason": user_presence_reason,
                    "status": "pending",
                }),
            )
        }

        "approve_sign_intent" => {
            cleanup_expired_intents(state);

            let Some(intent_id) = parse_string(&req.params, "intent_id").map(str::to_string) else {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "missing required string param: intent_id",
                );
            };
            let Some(challenge) = parse_string(&req.params, "challenge") else {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "missing required string param: challenge",
                );
            };

            let mut intents = match state.sign_intents.lock() {
                Ok(v) => v,
                Err(_) => {
                    return BrokerResponse::err(
                        id,
                        "ERR_STATE",
                        "failed to acquire sign intent lock",
                    )
                }
            };

            let Some(intent) = intents.get_mut(&intent_id) else {
                return BrokerResponse::err(id, "ERR_INTENT_NOT_FOUND", "sign intent not found");
            };

            if intent.challenge != challenge {
                return BrokerResponse::err(id, "ERR_AUTH", "invalid sign intent challenge");
            }

            if intent.expires_at_ms <= now_ms() {
                intents.remove(&intent_id);
                return BrokerResponse::err(id, "ERR_INTENT_EXPIRED", "sign intent expired");
            }

            if intent.require_user_presence {
                let reason = intent
                    .user_presence_reason
                    .as_deref()
                    .unwrap_or(DEFAULT_USER_PRESENCE_REASON);
                let allow_fallback = parse_bool(&req.params, "allow_fallback", true);

                let presence = match request_user_presence(reason, allow_fallback) {
                    Ok(v) => v,
                    Err(e) => return BrokerResponse::err(id, "ERR_AUTH", e),
                };

                intent.user_presence_verified = presence
                    .get("verified")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                intent.user_presence_modality = presence
                    .get("modality")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
            }

            intent.approved = true;

            BrokerResponse::ok(
                id,
                json!({
                    "intent_id": intent_id,
                    "approved": true,
                    "expires_at_ms": intent.expires_at_ms,
                    "require_user_presence": intent.require_user_presence,
                    "user_presence_verified": intent.user_presence_verified,
                    "user_presence_modality": intent.user_presence_modality,
                    "status": "approved",
                }),
            )
        }

        "consume_sign_intent" => {
            cleanup_expired_intents(state);

            let Some(intent_id) = parse_string(&req.params, "intent_id").map(str::to_string) else {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "missing required string param: intent_id",
                );
            };
            let Some(challenge) = parse_string(&req.params, "challenge") else {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "missing required string param: challenge",
                );
            };

            let mut intents = match state.sign_intents.lock() {
                Ok(v) => v,
                Err(_) => {
                    return BrokerResponse::err(
                        id,
                        "ERR_STATE",
                        "failed to acquire sign intent lock",
                    )
                }
            };

            let Some(intent) = intents.remove(&intent_id) else {
                return BrokerResponse::err(id, "ERR_INTENT_NOT_FOUND", "sign intent not found");
            };

            if intent.challenge != challenge {
                return BrokerResponse::err(id, "ERR_AUTH", "invalid sign intent challenge");
            }

            if intent.expires_at_ms <= now_ms() {
                return BrokerResponse::err(id, "ERR_INTENT_EXPIRED", "sign intent expired");
            }

            if !intent.approved {
                return BrokerResponse::err(
                    id,
                    "ERR_INTENT_NOT_APPROVED",
                    "sign intent not approved",
                );
            }

            BrokerResponse::ok(
                id,
                json!({
                    "intent_id": intent_id,
                    "account_id": intent.account_id,
                    "origin": intent.origin,
                    "payload": intent.payload,
                    "created_at_ms": intent.created_at_ms,
                    "expires_at_ms": intent.expires_at_ms,
                    "require_user_presence": intent.require_user_presence,
                    "user_presence_verified": intent.user_presence_verified,
                    "user_presence_modality": intent.user_presence_modality,
                    "status": "consumed",
                }),
            )
        }

        "sign_transaction" => {
            // Parse signer_id
            let Some(signer_id_str) = parse_string(&req.params, "signer_id") else {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "missing required string param: signer_id",
                );
            };
            let signer_id: AccountId = match signer_id_str.parse() {
                Ok(v) => v,
                Err(e) => {
                    return BrokerResponse::err(id, "ERR_PARAMS", format!("invalid signer_id: {e}"))
                }
            };

            // Parse receiver_id
            let Some(receiver_id_str) = parse_string(&req.params, "receiver_id") else {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "missing required string param: receiver_id",
                );
            };
            let receiver_id: AccountId = match receiver_id_str.parse() {
                Ok(v) => v,
                Err(e) => {
                    return BrokerResponse::err(
                        id,
                        "ERR_PARAMS",
                        format!("invalid receiver_id: {e}"),
                    )
                }
            };

            // Parse nonce
            let nonce = req
                .params
                .get("nonce")
                .and_then(Value::as_u64)
                .ok_or("missing required param: nonce");
            let nonce = match nonce {
                Ok(v) => v,
                Err(e) => return BrokerResponse::err(id, "ERR_PARAMS", e),
            };

            // Parse block_hash (base58)
            let Some(block_hash_str) = parse_string(&req.params, "block_hash") else {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "missing required string param: block_hash",
                );
            };
            let block_hash: CryptoHash = match block_hash_str.parse() {
                Ok(v) => v,
                Err(e) => {
                    return BrokerResponse::err(
                        id,
                        "ERR_PARAMS",
                        format!("invalid block_hash (expected base58): {e}"),
                    )
                }
            };

            // Parse actions array
            let actions_arr = match req.params.get("actions").and_then(Value::as_array) {
                Some(arr) if !arr.is_empty() => arr,
                _ => {
                    return BrokerResponse::err(
                        id,
                        "ERR_PARAMS",
                        "missing or empty 'actions' array",
                    )
                }
            };
            let actions = match parse_near_actions(actions_arr) {
                Ok(v) => v,
                Err(e) => return BrokerResponse::err(id, "ERR_PARAMS", e),
            };

            // Resolve credential from nearxd keychain and/or near-cli secure keychain.
            let network = parse_string(&req.params, "network")
                .unwrap_or("mainnet")
                .to_ascii_lowercase();
            let reason = parse_string(&req.params, "reason")
                .unwrap_or("NEARx needs your approval to sign a transaction.");
            let signer_public_key = parse_string(&req.params, "signer_public_key")
                .or_else(|| parse_string(&req.params, "public_key"));
            let credential_source_raw = parse_string(&req.params, "credential_source");
            let credential_source = match credential_source_raw {
                Some(raw) => match normalize_source(raw) {
                    Some(source) => Some(source.to_string()),
                    None => {
                        return BrokerResponse::err(
                            id,
                            "ERR_PARAMS",
                            format!("unsupported credential_source '{raw}'"),
                        )
                    }
                },
                None => None,
            };

            let mut secret_key: Option<SecretKey> = None;
            let mut hardware_record: Option<HardwareWalletIndexRecord> = None;
            let used_credential_source: String;
            let public_key: PublicKey;

            if credential_source.as_deref() == Some(SOURCE_HARDWARE_WALLET) {
                let Some(public_key_str) = signer_public_key else {
                    return BrokerResponse::err(
                        id,
                        "ERR_PARAMS",
                        "signer_public_key is required when credential_source=hardware_wallet",
                    );
                };
                public_key = match public_key_str.parse::<PublicKey>() {
                    Ok(v) => v,
                    Err(e) => {
                        return BrokerResponse::err(
                            id,
                            "ERR_PARAMS",
                            format!("invalid signer_public_key format: {e}"),
                        )
                    }
                };
                let record =
                    match resolve_hardware_wallet_record(&network, signer_id_str, public_key_str) {
                        Ok(v) => v,
                        Err(e) => return BrokerResponse::err(id, e.code, e.message),
                    };
                used_credential_source = SOURCE_HARDWARE_WALLET.to_string();
                hardware_record = Some(record);
            } else {
                let (credential, used_source) = match resolve_signing_credential(
                    &network,
                    signer_id_str,
                    signer_public_key,
                    credential_source.as_deref(),
                    reason,
                ) {
                    Ok(v) => v,
                    Err(e) => {
                        return BrokerResponse::err(
                            id,
                            "ERR_AUTH",
                            format!("credential read failed: {e}"),
                        )
                    }
                };
                used_credential_source = used_source;

                let Some(private_key_str) = credential.get("private_key").and_then(Value::as_str)
                else {
                    return BrokerResponse::err(
                        id,
                        "ERR_AUTH",
                        "credential missing 'private_key' field",
                    );
                };
                let parsed_secret: SecretKey = match private_key_str.parse() {
                    Ok(v) => v,
                    Err(e) => {
                        return BrokerResponse::err(
                            id,
                            "ERR_AUTH",
                            format!("invalid private key format: {e}"),
                        )
                    }
                };
                public_key = match credential
                    .get("public_key")
                    .and_then(Value::as_str)
                    .map(str::parse)
                {
                    Some(Ok(v)) => v,
                    _ => parsed_secret.public_key(),
                };
                secret_key = Some(parsed_secret);
            }

            // Build transaction
            let tx = Transaction::V0(TransactionV0 {
                signer_id: signer_id.clone(),
                public_key: public_key.clone(),
                nonce,
                receiver_id: receiver_id.clone(),
                block_hash,
                actions,
            });

            // Sign
            let signature = if let Some(secret) = secret_key {
                let (tx_hash, _size) = tx.get_hash_and_size();
                secret.sign(tx_hash.as_ref())
            } else {
                let record = hardware_record.expect("hardware source checked above");
                let unsigned_tx = match borsh::to_vec(&tx) {
                    Ok(v) => v,
                    Err(e) => {
                        return BrokerResponse::err(
                            id,
                            "ERR_INTERNAL",
                            format!("borsh serialization failed: {e}"),
                        )
                    }
                };
                match hardware_wallet_sign_transaction(
                    &record.wallet_type,
                    &record.derivation_path,
                    &unsigned_tx,
                ) {
                    Ok(sig) => sig,
                    Err(e) => return BrokerResponse::err(id, e.code, e.message),
                }
            };
            let (tx_hash, _size) = tx.get_hash_and_size();
            let signed_tx = SignedTransaction::new(signature, tx);

            // Serialize to borsh -> base64
            let borsh_bytes = match borsh::to_vec(&signed_tx) {
                Ok(v) => v,
                Err(e) => {
                    return BrokerResponse::err(
                        id,
                        "ERR_INTERNAL",
                        format!("borsh serialization failed: {e}"),
                    )
                }
            };
            let signed_tx_b64 = STANDARD.encode(&borsh_bytes);

            BrokerResponse::ok(
                id,
                json!({
                    "signed_transaction_base64": signed_tx_b64,
                    "tx_hash": tx_hash.to_string(),
                    "signer_id": signer_id.to_string(),
                    "public_key": public_key.to_string(),
                    "credential_source": used_credential_source,
                }),
            )
        }

        _ => BrokerResponse::err(id, "ERR_METHOD", format!("unknown method: {}", req.method)),
    }
}
