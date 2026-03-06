use near_crypto::SecretKey;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::keychain::{
    keychain_has_generic, keychain_list_generic_accounts, keychain_read_generic,
    keychain_read_generic_with_prompt, keychain_write_generic_protected,
};
use crate::util::now_ms;

pub(crate) const KEYCHAIN_NEAR_CREDENTIAL_SERVICE: &str = "nearxd.near.credentials";
pub(crate) const SOURCE_NEARXD_KEYCHAIN: &str = "nearxd_keychain";
pub(crate) const SOURCE_NEAR_CLI_SECURE: &str = "near_cli_secure";
pub(crate) const SOURCE_LEGACY_FILE: &str = "legacy_file";
pub(crate) const SOURCE_HARDWARE_WALLET: &str = "hardware_wallet";
pub(crate) const DEFAULT_KEYCHAIN_CREDENTIAL_PROTECTION: &str = "biometry_current_set";

#[derive(Debug, Deserialize)]
pub(crate) struct NearCredentialFile {
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub public_key: Option<String>,
    #[serde(default)]
    pub private_key: Option<String>,
    #[serde(default)]
    pub secret_key: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedNearCredential {
    pub account_id: String,
    pub public_key: String,
    pub private_key: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedLegacyCredential {
    pub credential: ParsedNearCredential,
    pub path: PathBuf,
}

pub(crate) fn credential_curve_type(public_key: &str) -> &'static str {
    if public_key.starts_with("ed25519:") {
        "ed25519"
    } else if public_key.starts_with("secp256k1:") {
        "secp256k1"
    } else {
        "unknown"
    }
}

pub(crate) fn nearxd_credential_account_key(
    network: &str,
    account_id: &str,
    public_key: &str,
) -> String {
    format!("{network}:{account_id}:{public_key}")
}

pub(crate) fn nearxd_credential_account_legacy(network: &str, account_id: &str) -> String {
    format!("{network}:{account_id}")
}

fn near_cli_secure_service(network: &str, account_id: &str) -> String {
    format!("near-{network}-{account_id}")
}

fn near_cli_secure_account(account_id: &str, public_key: &str) -> String {
    format!("{account_id}:{public_key}")
}

fn parse_near_cli_secure_public_key(account_id: &str, keychain_account: &str) -> Option<String> {
    let prefix = format!("{account_id}:");
    let public_key = keychain_account.strip_prefix(&prefix)?.trim();
    if public_key.is_empty() {
        return None;
    }
    Some(public_key.to_string())
}

pub(crate) fn near_cli_secure_public_keys_for_account(
    network: &str,
    account_id: &str,
) -> Vec<String> {
    let service = near_cli_secure_service(network, account_id);
    let accounts = match keychain_list_generic_accounts(&service) {
        Ok(v) => v,
        Err(e) => {
            log::warn!(
                "nearxd: failed to enumerate near-cli secure keychain accounts for {}:{}: {}",
                network,
                account_id,
                e
            );
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for account in accounts {
        if let Some(public_key) = parse_near_cli_secure_public_key(account_id, &account) {
            out.push(public_key);
        }
    }
    out.sort();
    out.dedup();
    out
}

pub(crate) fn parse_near_credential_from_value(
    parsed: NearCredentialFile,
    fallback_account: &str,
    path_display: &str,
) -> Result<ParsedNearCredential, String> {
    let account_id = parsed
        .account_id
        .unwrap_or_else(|| fallback_account.to_string())
        .trim()
        .to_string();
    if account_id.is_empty() {
        return Err(format!(
            "missing account_id in credential source {path_display}"
        ));
    }

    let public_key = parsed.public_key.unwrap_or_default().trim().to_string();
    if public_key.is_empty() {
        return Err(format!(
            "missing public_key in credential source {path_display}"
        ));
    }

    let private_key = parsed
        .private_key
        .or(parsed.secret_key)
        .unwrap_or_default()
        .trim()
        .to_string();
    if private_key.is_empty() {
        return Err(format!(
            "missing private_key in credential source {path_display}"
        ));
    }

    Ok(ParsedNearCredential {
        account_id,
        public_key,
        private_key,
    })
}

pub(crate) fn parse_near_credential(path: &Path) -> Result<ParsedNearCredential, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let parsed: NearCredentialFile =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;

    let fallback_account = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_default();

    parse_near_credential_from_value(parsed, &fallback_account, &path.display().to_string())
}

pub(crate) fn collect_credential_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("read_dir entry error: {e}"))?;
        let path = entry.path();
        if path.is_file() {
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                files.push(path);
            }
            continue;
        }
        // near-cli legacy layout: <credentials_dir>/<network>/<account_id>/<public_key>.json
        if !path.is_dir() {
            continue;
        }
        let nested =
            fs::read_dir(&path).map_err(|e| format!("read_dir nested {}: {e}", path.display()))?;
        for nested_entry in nested {
            let nested_entry = nested_entry.map_err(|e| format!("read_dir entry error: {e}"))?;
            let nested_path = nested_entry.path();
            if !nested_path.is_file() {
                continue;
            }
            if nested_path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            files.push(nested_path);
        }
    }

    files.sort();
    Ok(files)
}

pub(crate) fn collect_legacy_credentials(
    dir: &Path,
) -> Result<Vec<ParsedLegacyCredential>, String> {
    let files = collect_credential_files(dir)?;
    let mut out = Vec::new();
    let mut dedupe: HashSet<(String, String)> = HashSet::new();
    for path in files {
        let cred = match parse_near_credential(&path) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let key = (cred.account_id.clone(), cred.public_key.clone());
        if dedupe.contains(&key) {
            continue;
        }
        dedupe.insert(key);
        out.push(ParsedLegacyCredential {
            credential: cred,
            path,
        });
    }
    Ok(out)
}

pub(crate) fn nearxd_keychain_has_credential(
    network: &str,
    account_id: &str,
    public_key: &str,
) -> bool {
    let scoped = nearxd_credential_account_key(network, account_id, public_key);
    keychain_has_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &scoped)
}

pub(crate) fn near_cli_secure_has_credential(
    network: &str,
    account_id: &str,
    public_key: &str,
) -> bool {
    let service = near_cli_secure_service(network, account_id);
    let account = near_cli_secure_account(account_id, public_key);
    keychain_has_generic(&service, &account)
}

pub(crate) fn store_near_credential_keychain(
    network: &str,
    cred: &ParsedNearCredential,
    overwrite: bool,
    protection: &str,
) -> Result<&'static str, String> {
    if !cfg!(target_os = "macos") {
        return Err("credential keychain storage is only supported on macOS".to_string());
    }

    let account = nearxd_credential_account_key(network, &cred.account_id, &cred.public_key);
    if !overwrite && keychain_has_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &account) {
        return Ok("skipped_existing");
    }

    let payload = json!({
        "network": network,
        "account_id": cred.account_id,
        "public_key": cred.public_key,
        "private_key": cred.private_key,
        "imported_at_ms": now_ms(),
    });
    let encoded =
        serde_json::to_string(&payload).map_err(|e| format!("encode credential payload: {e}"))?;
    let biometry_only = match protection {
        "biometry_current_set" => true,
        "user_presence" => false,
        _ => {
            return Err(format!(
                "unsupported keychain credential protection mode: {protection}"
            ))
        }
    };

    keychain_write_generic_protected(
        KEYCHAIN_NEAR_CREDENTIAL_SERVICE,
        &account,
        &encoded,
        biometry_only,
    )?;
    Ok(if biometry_only {
        "stored_biometry_current_set"
    } else {
        "stored_user_presence"
    })
}

pub(crate) fn read_near_credential_keychain(
    network: &str,
    account_id: &str,
    signer_public_key: Option<&str>,
    reason: &str,
) -> Result<Value, String> {
    if !cfg!(target_os = "macos") {
        return Err("credential keychain reads are only supported on macOS".to_string());
    }

    let (raw, keychain_account) = if let Some(public_key) = signer_public_key {
        let scoped_account = nearxd_credential_account_key(network, account_id, public_key);
        match keychain_read_generic_with_prompt(
            KEYCHAIN_NEAR_CREDENTIAL_SERVICE,
            &scoped_account,
            reason,
        ) {
            Ok(v) => (v, scoped_account),
            Err(_) => {
                // Backward compatibility for prior account-only namespace.
                let legacy_account = nearxd_credential_account_legacy(network, account_id);
                let raw = keychain_read_generic_with_prompt(
                    KEYCHAIN_NEAR_CREDENTIAL_SERVICE,
                    &legacy_account,
                    reason,
                )?;
                (raw, legacy_account)
            }
        }
    } else {
        let legacy_account = nearxd_credential_account_legacy(network, account_id);
        (
            keychain_read_generic_with_prompt(
                KEYCHAIN_NEAR_CREDENTIAL_SERVICE,
                &legacy_account,
                reason,
            )?,
            legacy_account,
        )
    };

    let mut payload =
        serde_json::from_str::<Value>(&raw).map_err(|e| format!("decode stored payload: {e}"))?;

    if let Some(requested_public_key) = signer_public_key {
        let payload_public_key = payload
            .get("public_key")
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .or_else(|| {
                payload
                    .get("private_key")
                    .and_then(Value::as_str)
                    .and_then(|raw_sk| raw_sk.parse::<SecretKey>().ok())
                    .map(|sk| sk.public_key().to_string())
            });
        let matched = payload_public_key
            .as_deref()
            .map(|pk| pk == requested_public_key)
            .unwrap_or(false);
        if !matched {
            return Err(format!(
                "credential in keychain account '{}' does not match requested public_key '{}'",
                keychain_account, requested_public_key
            ));
        }
    }

    if let Some(obj) = payload.as_object_mut() {
        obj.insert("keychain_account".to_string(), json!(keychain_account));
        obj.insert("source".to_string(), json!(SOURCE_NEARXD_KEYCHAIN));
    }
    Ok(payload)
}

pub(crate) fn read_near_cli_secure_credential(
    network: &str,
    account_id: &str,
    public_key: &str,
) -> Result<Value, String> {
    if !cfg!(target_os = "macos") {
        return Err("near-cli secure keychain reads are only supported on macOS".to_string());
    }

    let service = near_cli_secure_service(network, account_id);
    let account = near_cli_secure_account(account_id, public_key);
    let raw = keychain_read_generic(&service, &account).ok_or_else(|| {
        format!(
            "near-cli secure keychain credential not found for {network}:{account_id}:{public_key}"
        )
    })?;
    let parsed: NearCredentialFile = serde_json::from_str(&raw)
        .map_err(|e| format!("decode near-cli credential payload: {e}"))?;
    let cred = parse_near_credential_from_value(
        parsed,
        account_id,
        &format!("near-cli secure keychain {} / {}", service, account),
    )?;
    Ok(json!({
        "network": network,
        "account_id": cred.account_id,
        "public_key": cred.public_key,
        "private_key": cred.private_key,
        "source": SOURCE_NEAR_CLI_SECURE,
        "service": service,
        "keychain_account": account,
    }))
}

pub(crate) fn parse_credential_from_value(v: &Value) -> Result<ParsedNearCredential, String> {
    let account_id = v
        .get("account_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let public_key = v
        .get("public_key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let private_key = v
        .get("private_key")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if account_id.is_empty() || public_key.is_empty() || private_key.is_empty() {
        return Err("missing account_id/public_key/private_key in credential object".to_string());
    }
    Ok(ParsedNearCredential {
        account_id,
        public_key,
        private_key,
    })
}

pub(crate) fn normalize_source(source: &str) -> Option<&'static str> {
    match source.trim().to_ascii_lowercase().as_str() {
        SOURCE_NEARXD_KEYCHAIN => Some(SOURCE_NEARXD_KEYCHAIN),
        SOURCE_NEAR_CLI_SECURE => Some(SOURCE_NEAR_CLI_SECURE),
        SOURCE_LEGACY_FILE => Some(SOURCE_LEGACY_FILE),
        SOURCE_HARDWARE_WALLET => Some(SOURCE_HARDWARE_WALLET),
        _ => None,
    }
}

pub(crate) fn normalize_sources(sources: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for source in sources {
        if let Some(v) = normalize_source(source) {
            if !out.iter().any(|s: &String| s == v) {
                out.push(v.to_string());
            }
        }
    }
    out
}
