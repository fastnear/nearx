use near_crypto::PublicKey;
use near_primitives::types::AccountId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::sync::Mutex;

use crate::config::{signing_settings_file_path, validate_account_id_param};
use crate::credentials::{normalize_keychain_protection, normalize_sources};
use crate::util::now_ms;

pub(crate) const SIGNING_KEY_INDEX_KEY: &str = "near_signing_keys";
pub(crate) const SIGNING_KEY_INDEX_VERSION: u64 = 1;
pub(crate) const SIGNING_KEY_LABEL_MAX_CHARS: usize = 64;
pub(crate) const SIGNING_KEY_STALE_AFTER_MS: u64 = 90 * 24 * 60 * 60 * 1000; // 90 days
pub(crate) const SIGNING_KEY_PRUNE_AFTER_MS: u64 = 365 * 24 * 60 * 60 * 1000; // 1 year
pub(crate) const PREFERENCES_KEY: &str = "preferences";
pub(crate) const PREFERENCES_VERSION: u64 = 1;
pub(crate) const STAKING_WATCHLIST_KEY: &str = "staking_watchlist";
pub(crate) const STAKING_WATCHLIST_VERSION: u64 = 1;
pub(crate) const HARDWARE_WALLET_INDEX_KEY: &str = "hardware_wallet_index";
pub(crate) const HARDWARE_WALLET_INDEX_VERSION: u64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct IndexedSigningKeyRecord {
    pub network: String,
    pub account_id: String,
    pub public_key: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub available_sources: Vec<String>,
    #[serde(default)]
    pub in_nearxd_keychain: bool,
    #[serde(default)]
    pub nearxd_keychain_protection: Option<String>,
    #[serde(default)]
    pub last_seen_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StakingWatchlistEntry {
    pub network: String,
    pub account_id: String,
    #[serde(default)]
    pub added_at_ms: u64,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub hardware_wallet: Option<StakingWatchlistHardwareWallet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StakingWatchlistHardwareWallet {
    pub wallet_type: String,
    pub public_key: String,
    pub derivation_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct HardwareWalletIndexRecord {
    pub network: String,
    pub account_id: String,
    pub public_key: String,
    pub wallet_type: String,
    pub derivation_path: String,
    #[serde(default)]
    pub last_seen_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct UserPreferences {
    #[serde(default)]
    pub always_prompt_user_presence: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_wasm_directory: Option<String>,
}

pub(crate) fn read_preferences(settings: &Value) -> UserPreferences {
    settings
        .get(PREFERENCES_KEY)
        .and_then(|v| v.get("data"))
        .and_then(|v| serde_json::from_value::<UserPreferences>(v.clone()).ok())
        .unwrap_or_default()
}

pub(crate) fn write_preferences(settings: &mut Value, prefs: &UserPreferences) {
    if !settings.is_object() {
        *settings = json!({});
    }
    if let Some(obj) = settings.as_object_mut() {
        obj.insert(
            PREFERENCES_KEY.to_string(),
            json!({
                "version": PREFERENCES_VERSION,
                "data": serde_json::to_value(prefs).unwrap_or_else(|_| json!({})),
            }),
        );
    }
}

pub(crate) fn is_index_record_stale(last_seen_at_ms: u64, now: u64) -> bool {
    if last_seen_at_ms == 0 {
        return true;
    }
    now.saturating_sub(last_seen_at_ms) > SIGNING_KEY_STALE_AFTER_MS
}

pub(crate) fn is_index_record_prunable(last_seen_at_ms: u64, now: u64) -> bool {
    if last_seen_at_ms == 0 {
        return false;
    }
    now.saturating_sub(last_seen_at_ms) > SIGNING_KEY_PRUNE_AFTER_MS
}

fn normalize_signing_key_label_value(label: Option<String>) -> Option<String> {
    label.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_staking_watchlist_hardware_wallet(
    hardware_wallet: Option<StakingWatchlistHardwareWallet>,
) -> Option<StakingWatchlistHardwareWallet> {
    let mut hardware_wallet = hardware_wallet?;
    hardware_wallet.wallet_type = hardware_wallet.wallet_type.trim().to_ascii_lowercase();
    hardware_wallet.public_key = hardware_wallet.public_key.trim().to_string();
    hardware_wallet.derivation_path = hardware_wallet.derivation_path.trim().to_string();
    if hardware_wallet.wallet_type != "ledger"
        || hardware_wallet.public_key.parse::<PublicKey>().is_err()
        || hardware_wallet.derivation_path.is_empty()
        || hardware_wallet
            .derivation_path
            .parse::<slipped10::BIP32Path>()
            .is_err()
    {
        return None;
    }
    Some(hardware_wallet)
}

fn parse_staking_watchlist_hardware_wallet(
    params: &Value,
) -> Result<Option<StakingWatchlistHardwareWallet>, String> {
    let wallet_type = crate::broker::parse_string(params, "wallet_type");
    let public_key = crate::broker::parse_string(params, "public_key");
    let derivation_path = crate::broker::parse_string(params, "derivation_path");
    if wallet_type.is_none() && public_key.is_none() && derivation_path.is_none() {
        return Ok(None);
    }

    let Some(wallet_type) = wallet_type else {
        return Err(
            "wallet_type is required when hardware wallet metadata is provided".to_string(),
        );
    };
    let Some(public_key) = public_key else {
        return Err("public_key is required when hardware wallet metadata is provided".to_string());
    };
    let Some(derivation_path) = derivation_path else {
        return Err(
            "derivation_path is required when hardware wallet metadata is provided".to_string(),
        );
    };

    let hardware_wallet =
        normalize_staking_watchlist_hardware_wallet(Some(StakingWatchlistHardwareWallet {
            wallet_type: wallet_type.to_string(),
            public_key: public_key.to_string(),
            derivation_path: derivation_path.to_string(),
        }));

    hardware_wallet.ok_or_else(|| {
        "hardware wallet metadata must contain a supported wallet_type, public_key, and derivation_path"
            .to_string()
    }).map(Some)
}

pub(crate) fn parse_signing_key_label(params: &Value, key: &str) -> Result<Option<String>, String> {
    let Some(value) = params.get(key) else {
        return Err(format!("missing required string param: {key}"));
    };
    if value.is_null() {
        return Ok(None);
    }
    let Some(raw) = value.as_str() else {
        return Err(format!("{key} must be a string or null"));
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > SIGNING_KEY_LABEL_MAX_CHARS {
        return Err(format!(
            "{key} must be {SIGNING_KEY_LABEL_MAX_CHARS} characters or fewer"
        ));
    }
    Ok(Some(trimmed.to_string()))
}

fn read_signing_settings_file() -> Option<Value> {
    let path = signing_settings_file_path()?;
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&raw).ok()
}

pub(crate) fn write_signing_settings_file(settings: &Value) -> Result<(), String> {
    let Some(path) = signing_settings_file_path() else {
        return Err("signing settings path unavailable".to_string());
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create settings directory: {e}"))?;
    }

    let body =
        serde_json::to_string_pretty(settings).map_err(|e| format!("encode settings json: {e}"))?;

    let tmp = path.with_extension("tmp");
    fs::write(&tmp, body.as_bytes()).map_err(|e| format!("write settings temp file: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&tmp, perms).map_err(|e| format!("chmod settings temp file: {e}"))?;
    }

    fs::rename(&tmp, &path).map_err(|e| format!("rename settings file: {e}"))?;

    Ok(())
}

pub(crate) fn read_signing_key_index(settings: &Value) -> Vec<IndexedSigningKeyRecord> {
    let Some(index) = settings.get(SIGNING_KEY_INDEX_KEY) else {
        return Vec::new();
    };
    let Some(records) = index.get("records").and_then(Value::as_array) else {
        return Vec::new();
    };
    records
        .iter()
        .filter_map(|v| serde_json::from_value::<IndexedSigningKeyRecord>(v.clone()).ok())
        .filter_map(|mut r| {
            r.label = normalize_signing_key_label_value(r.label);
            r.nearxd_keychain_protection = r
                .nearxd_keychain_protection
                .as_deref()
                .and_then(normalize_keychain_protection)
                .map(str::to_string);
            if r.network.is_empty() || r.account_id.is_empty() || r.public_key.is_empty() {
                return None;
            }
            Some(r)
        })
        .collect()
}

pub(crate) fn read_staking_watchlist(settings: &Value) -> Vec<StakingWatchlistEntry> {
    let Some(index) = settings.get(STAKING_WATCHLIST_KEY) else {
        return Vec::new();
    };
    let Some(entries) = index.get("entries").and_then(Value::as_array) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|v| serde_json::from_value::<StakingWatchlistEntry>(v.clone()).ok())
        .filter_map(|mut entry| {
            entry.network = entry.network.trim().to_ascii_lowercase();
            entry.account_id = entry.account_id.trim().to_string();
            entry.source = if entry.source.trim().is_empty() {
                "manual".to_string()
            } else {
                entry.source.trim().to_string()
            };
            entry.hardware_wallet =
                normalize_staking_watchlist_hardware_wallet(entry.hardware_wallet.take());
            if entry.network.is_empty() || entry.account_id.is_empty() {
                return None;
            }
            if entry.account_id.parse::<AccountId>().is_err() {
                return None;
            }
            Some(entry)
        })
        .collect()
}

fn write_staking_watchlist(settings: &mut Value, entries: &[StakingWatchlistEntry]) {
    if !settings.is_object() {
        *settings = json!({});
    }
    let mut out = entries.to_vec();
    out.sort_by(|a, b| {
        b.added_at_ms
            .cmp(&a.added_at_ms)
            .then_with(|| a.account_id.cmp(&b.account_id))
    });
    let values: Vec<Value> = out
        .iter()
        .filter_map(|entry| serde_json::to_value(entry).ok())
        .collect();
    if let Some(obj) = settings.as_object_mut() {
        obj.insert(
            STAKING_WATCHLIST_KEY.to_string(),
            json!({
                "version": STAKING_WATCHLIST_VERSION,
                "entries": values,
            }),
        );
    }
}

fn upsert_staking_watchlist_entry(settings: &mut Value, mut entry: StakingWatchlistEntry) {
    entry.network = entry.network.trim().to_ascii_lowercase();
    entry.account_id = entry.account_id.trim().to_string();
    if entry.source.trim().is_empty() {
        entry.source = "manual".to_string();
    }
    entry.hardware_wallet =
        normalize_staking_watchlist_hardware_wallet(entry.hardware_wallet.take());
    let mut entries = read_staking_watchlist(settings);
    entries.retain(|e| !(e.network == entry.network && e.account_id == entry.account_id));
    entries.push(entry);
    write_staking_watchlist(settings, &entries);
}

fn remove_staking_watchlist_entry(settings: &mut Value, network: &str, account_id: &str) -> bool {
    let mut entries = read_staking_watchlist(settings);
    let before = entries.len();
    entries.retain(|e| !(e.network == network && e.account_id == account_id));
    let removed = entries.len() != before;
    if removed {
        write_staking_watchlist(settings, &entries);
    }
    removed
}

pub(crate) fn staking_watchlist_for_network(
    settings: &Value,
    network: &str,
) -> Vec<StakingWatchlistEntry> {
    let mut out: Vec<StakingWatchlistEntry> = read_staking_watchlist(settings)
        .into_iter()
        .filter(|entry| entry.network == network)
        .collect();
    out.sort_by(|a, b| {
        b.added_at_ms
            .cmp(&a.added_at_ms)
            .then_with(|| a.account_id.cmp(&b.account_id))
    });
    out
}

pub(crate) fn read_hardware_wallet_index(settings: &Value) -> Vec<HardwareWalletIndexRecord> {
    let Some(index) = settings.get(HARDWARE_WALLET_INDEX_KEY) else {
        return Vec::new();
    };
    let Some(records) = index.get("records").and_then(Value::as_array) else {
        return Vec::new();
    };
    records
        .iter()
        .filter_map(|v| serde_json::from_value::<HardwareWalletIndexRecord>(v.clone()).ok())
        .filter_map(|mut r| {
            r.network = r.network.trim().to_ascii_lowercase();
            r.account_id = r.account_id.trim().to_string();
            r.public_key = r.public_key.trim().to_string();
            r.wallet_type = r.wallet_type.trim().to_ascii_lowercase();
            r.derivation_path = r.derivation_path.trim().to_string();
            if r.network.is_empty()
                || r.account_id.is_empty()
                || r.public_key.is_empty()
                || r.wallet_type.is_empty()
                || r.derivation_path.is_empty()
            {
                return None;
            }
            Some(r)
        })
        .collect()
}

pub(crate) fn upsert_hardware_wallet_index(
    settings: &mut Value,
    updates: &[HardwareWalletIndexRecord],
) {
    if updates.is_empty() {
        return;
    }
    if !settings.is_object() {
        *settings = json!({});
    }
    let mut merged: BTreeMap<(String, String, String), HardwareWalletIndexRecord> = BTreeMap::new();
    for record in read_hardware_wallet_index(settings) {
        merged.insert(
            (
                record.network.clone(),
                record.account_id.clone(),
                record.public_key.clone(),
            ),
            record,
        );
    }
    for update in updates {
        let key = (
            update.network.clone(),
            update.account_id.clone(),
            update.public_key.clone(),
        );
        let mut next = update.clone();
        if let Some(prev) = merged.get(&key) {
            next.last_seen_at_ms = next.last_seen_at_ms.max(prev.last_seen_at_ms);
        }
        merged.insert(key, next);
    }
    let records: Vec<Value> = merged
        .values()
        .filter_map(|record| serde_json::to_value(record).ok())
        .collect();
    if let Some(obj) = settings.as_object_mut() {
        obj.insert(
            HARDWARE_WALLET_INDEX_KEY.to_string(),
            json!({
                "version": HARDWARE_WALLET_INDEX_VERSION,
                "records": records,
            }),
        );
    }
}

pub(crate) fn hardware_wallet_record_for_key(
    settings: &Value,
    network: &str,
    account_id: &str,
    public_key: &str,
) -> Option<HardwareWalletIndexRecord> {
    read_hardware_wallet_index(settings)
        .into_iter()
        .find(|r| r.network == network && r.account_id == account_id && r.public_key == public_key)
}

pub(crate) fn hardware_wallet_records_for_account(
    settings: &Value,
    network: &str,
    account_id: &str,
) -> Vec<HardwareWalletIndexRecord> {
    let mut records: Vec<HardwareWalletIndexRecord> = read_hardware_wallet_index(settings)
        .into_iter()
        .filter(|r| r.network == network && r.account_id == account_id)
        .collect();
    records.sort_by(|a, b| {
        a.derivation_path
            .cmp(&b.derivation_path)
            .then_with(|| a.public_key.cmp(&b.public_key))
    });
    records
}

pub(crate) fn upsert_signing_key_index(settings: &mut Value, updates: &[IndexedSigningKeyRecord]) {
    if updates.is_empty() {
        return;
    }
    if !settings.is_object() {
        *settings = json!({});
    }
    let existing = read_signing_key_index(settings);
    let now = now_ms();
    let mut merged: BTreeMap<(String, String, String), IndexedSigningKeyRecord> = BTreeMap::new();

    for mut record in existing {
        if is_index_record_prunable(record.last_seen_at_ms, now) {
            continue;
        }
        record.available_sources = normalize_sources(&record.available_sources);
        record.label = normalize_signing_key_label_value(record.label);
        merged.insert(
            (
                record.network.clone(),
                record.account_id.clone(),
                record.public_key.clone(),
            ),
            record,
        );
    }

    for update in updates {
        let key = (
            update.network.clone(),
            update.account_id.clone(),
            update.public_key.clone(),
        );
        let mut next = update.clone();
        next.available_sources = normalize_sources(&next.available_sources);
        next.label = normalize_signing_key_label_value(next.label);
        next.nearxd_keychain_protection = next
            .nearxd_keychain_protection
            .as_deref()
            .and_then(normalize_keychain_protection)
            .map(str::to_string);
        if let Some(prev) = merged.get(&key) {
            let mut joined = prev.available_sources.clone();
            for source in &next.available_sources {
                if !joined.iter().any(|s| s == source) {
                    joined.push(source.clone());
                }
            }
            next.available_sources = joined;
            if next.label.is_none() {
                next.label = prev.label.clone();
            }
            if next.nearxd_keychain_protection.is_none() {
                next.nearxd_keychain_protection = prev.nearxd_keychain_protection.clone();
            }
            next.in_nearxd_keychain |= prev.in_nearxd_keychain;
            next.last_seen_at_ms = next.last_seen_at_ms.max(prev.last_seen_at_ms);
        }
        merged.insert(key, next);
    }

    let records: Vec<Value> = merged
        .values()
        .filter_map(|r| serde_json::to_value(r).ok())
        .collect();
    let index = json!({
        "version": SIGNING_KEY_INDEX_VERSION,
        "records": records,
    });
    if let Some(obj) = settings.as_object_mut() {
        obj.insert(SIGNING_KEY_INDEX_KEY.to_string(), index);
    }
}

pub(crate) fn indexed_signing_keys_for_network(
    settings: &Value,
    network: &str,
) -> Vec<IndexedSigningKeyRecord> {
    let now = now_ms();
    read_signing_key_index(settings)
        .into_iter()
        .filter(|r| r.network == network && !is_index_record_prunable(r.last_seen_at_ms, now))
        .collect()
}

pub(crate) fn signing_key_label(
    settings: &Value,
    network: &str,
    account_id: &str,
    public_key: &str,
) -> Option<String> {
    indexed_signing_keys_for_network(settings, network)
        .into_iter()
        .find(|record| record.account_id == account_id && record.public_key == public_key)
        .and_then(|record| record.label)
}

pub(crate) fn load_signing_settings() -> (Option<Value>, &'static str) {
    // File is the sole storage for settings (non-secret metadata).
    if let Some(v) = read_signing_settings_file() {
        return (Some(v), "file");
    }
    (None, "none")
}

pub(crate) fn persist_signing_settings(
    settings: &Value,
) -> Result<&'static str, String> {
    write_signing_settings_file(settings)?;
    Ok("file")
}

pub(crate) fn set_signing_key_label_result(
    params: &Value,
    settings_lock: &Mutex<()>,
) -> Result<Value, (&'static str, String)> {
    let network =
        crate::broker::resolve_network_param(params, "mainnet").map_err(|e| ("ERR_PARAMS", e))?;
    let Some(raw_account_id) = crate::broker::parse_string(params, "account_id") else {
        return Err((
            "ERR_PARAMS",
            "missing required string param: account_id".to_string(),
        ));
    };
    let account_id = validate_account_id_param(raw_account_id).map_err(|e| ("ERR_PARAMS", e))?;
    let Some(raw_public_key) = crate::broker::parse_string(params, "public_key") else {
        return Err((
            "ERR_PARAMS",
            "missing required string param: public_key".to_string(),
        ));
    };
    let public_key = raw_public_key
        .parse::<PublicKey>()
        .map_err(|e| ("ERR_PARAMS", format!("invalid public_key: {e}")))?
        .to_string();
    let label = parse_signing_key_label(params, "label").map_err(|e| ("ERR_PARAMS", e))?;

    let _guard = settings_lock.lock().unwrap();
    let mut settings = load_signing_settings().0.unwrap_or_else(|| json!({}));
    if !settings.is_object() {
        settings = json!({});
    }

    upsert_signing_key_index(
        &mut settings,
        &[IndexedSigningKeyRecord {
            network: network.clone(),
            account_id: account_id.clone(),
            public_key: public_key.clone(),
            label: label.clone(),
            available_sources: Vec::new(),
            in_nearxd_keychain: false,
            nearxd_keychain_protection: None,
            last_seen_at_ms: now_ms(),
        }],
    );
    let settings_store =
        persist_signing_settings(&settings).map_err(|e| ("ERR_PERSIST", e))?;
    drop(_guard);

    Ok(json!({
        "network": network,
        "account_id": account_id,
        "public_key": public_key,
        "label": label,
        "settings_save": {
            "saved": true,
            "source": settings_store,
        },
    }))
}

// Staking watchlist result handlers

pub(crate) fn list_staking_watchlist_result(
    params: &Value,
) -> Result<Value, (&'static str, String)> {
    let network =
        crate::broker::resolve_network_param(params, "mainnet").map_err(|e| ("ERR_PARAMS", e))?;
    let (settings, settings_source) = load_signing_settings();
    let entries = settings
        .as_ref()
        .map(|s| staking_watchlist_for_network(s, &network))
        .unwrap_or_default();
    let out_entries: Vec<Value> = entries
        .iter()
        .filter_map(|entry| serde_json::to_value(entry).ok())
        .collect();
    Ok(json!({
        "network": network,
        "entries": out_entries,
        "settings_source": settings_source,
    }))
}

pub(crate) fn add_staking_watchlist_account_result(
    params: &Value,
    settings_lock: &Mutex<()>,
) -> Result<Value, (&'static str, String)> {
    let network =
        crate::broker::resolve_network_param(params, "mainnet").map_err(|e| ("ERR_PARAMS", e))?;
    let Some(raw_account_id) = crate::broker::parse_string(params, "account_id") else {
        return Err((
            "ERR_PARAMS",
            "missing required string param: account_id".to_string(),
        ));
    };
    let account_id = validate_account_id_param(raw_account_id).map_err(|e| ("ERR_PARAMS", e))?;
    let source = crate::broker::parse_string(params, "source")
        .map(str::to_string)
        .unwrap_or_else(|| "manual".to_string());
    if source != "manual" && source != "seeded" && source != "hardware_wallet" {
        return Err((
            "ERR_PARAMS",
            "source must be 'manual', 'seeded', or 'hardware_wallet'".to_string(),
        ));
    }
    let hardware_wallet =
        parse_staking_watchlist_hardware_wallet(params).map_err(|e| ("ERR_PARAMS", e))?;
    if source == "hardware_wallet" && hardware_wallet.is_none() {
        return Err((
            "ERR_PARAMS",
            "hardware_wallet source requires wallet_type, public_key, and derivation_path"
                .to_string(),
        ));
    }
    if source != "hardware_wallet" && hardware_wallet.is_some() {
        return Err((
            "ERR_PARAMS",
            "hardware wallet metadata is only allowed when source='hardware_wallet'".to_string(),
        ));
    }
    let _guard = settings_lock.lock().unwrap();
    let mut settings = load_signing_settings().0.unwrap_or_else(|| json!({}));
    if !settings.is_object() {
        settings = json!({});
    }
    let entry = StakingWatchlistEntry {
        network: network.clone(),
        account_id: account_id.clone(),
        added_at_ms: now_ms(),
        source,
        hardware_wallet,
    };
    upsert_staking_watchlist_entry(&mut settings, entry);
    let settings_store =
        persist_signing_settings(&settings).map_err(|e| ("ERR_PERSIST", e))?;

    let entries = staking_watchlist_for_network(&settings, &network);
    drop(_guard);
    let out_entries: Vec<Value> = entries
        .iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    Ok(json!({
        "network": network,
        "account_id": account_id,
        "entries": out_entries,
        "settings_save": {
            "saved": true,
            "source": settings_store,
        },
    }))
}

pub(crate) fn remove_staking_watchlist_account_result(
    params: &Value,
    settings_lock: &Mutex<()>,
) -> Result<Value, (&'static str, String)> {
    let network =
        crate::broker::resolve_network_param(params, "mainnet").map_err(|e| ("ERR_PARAMS", e))?;
    let Some(raw_account_id) = crate::broker::parse_string(params, "account_id") else {
        return Err((
            "ERR_PARAMS",
            "missing required string param: account_id".to_string(),
        ));
    };
    let account_id = validate_account_id_param(raw_account_id).map_err(|e| ("ERR_PARAMS", e))?;

    let _guard = settings_lock.lock().unwrap();
    let mut settings = load_signing_settings().0.unwrap_or_else(|| json!({}));
    if !settings.is_object() {
        settings = json!({});
    }
    let removed = remove_staking_watchlist_entry(&mut settings, &network, &account_id);
    let settings_store =
        persist_signing_settings(&settings).map_err(|e| ("ERR_PERSIST", e))?;

    let entries = staking_watchlist_for_network(&settings, &network);
    drop(_guard);
    let out_entries: Vec<Value> = entries
        .iter()
        .filter_map(|e| serde_json::to_value(e).ok())
        .collect();
    Ok(json!({
        "network": network,
        "account_id": account_id,
        "removed": removed,
        "entries": out_entries,
        "settings_save": {
            "saved": true,
            "source": settings_store,
        },
    }))
}
