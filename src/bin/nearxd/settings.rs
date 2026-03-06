use near_primitives::types::AccountId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;

use crate::config::{signing_settings_file_path, validate_account_id_param};
use crate::credentials::normalize_sources;
use crate::keychain::{keychain_read_generic, keychain_write_generic};
use crate::util::now_ms;

pub(crate) const KEYCHAIN_SIGNING_SETTINGS_SERVICE: &str = "nearxd.signing.settings";
pub(crate) const KEYCHAIN_SIGNING_SETTINGS_ACCOUNT: &str = "default";
pub(crate) const SIGNING_KEY_INDEX_KEY: &str = "near_signing_keys";
pub(crate) const SIGNING_KEY_INDEX_VERSION: u64 = 1;
pub(crate) const SIGNING_KEY_STALE_AFTER_MS: u64 = 90 * 24 * 60 * 60 * 1000; // 90 days
pub(crate) const SIGNING_KEY_PRUNE_AFTER_MS: u64 = 365 * 24 * 60 * 60 * 1000; // 1 year
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
    pub available_sources: Vec<String>,
    #[serde(default)]
    pub in_nearxd_keychain: bool,
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
    fs::write(&path, body.as_bytes()).map_err(|e| format!("write settings file: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms).map_err(|e| format!("chmod settings file: {e}"))?;
    }

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
        .filter(|r| !r.network.is_empty() && !r.account_id.is_empty() && !r.public_key.is_empty())
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
        if let Some(prev) = merged.get(&key) {
            let mut joined = prev.available_sources.clone();
            for source in &next.available_sources {
                if !joined.iter().any(|s| s == source) {
                    joined.push(source.clone());
                }
            }
            next.available_sources = joined;
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

pub(crate) fn load_signing_settings() -> (Option<Value>, &'static str) {
    if let Some(raw) = keychain_read_generic(
        KEYCHAIN_SIGNING_SETTINGS_SERVICE,
        KEYCHAIN_SIGNING_SETTINGS_ACCOUNT,
    ) {
        if let Ok(v) = serde_json::from_str::<Value>(&raw) {
            return (Some(v), "keychain");
        }
        log::warn!("nearxd: failed to parse keychain signing settings JSON");
    }

    if let Some(v) = read_signing_settings_file() {
        return (Some(v), "file");
    }

    (None, "none")
}

pub(crate) fn persist_signing_settings(
    settings: &Value,
    prefer_keychain: bool,
) -> Result<&'static str, String> {
    let encoded =
        serde_json::to_string(settings).map_err(|e| format!("encode settings json: {e}"))?;

    if prefer_keychain {
        match keychain_write_generic(
            KEYCHAIN_SIGNING_SETTINGS_SERVICE,
            KEYCHAIN_SIGNING_SETTINGS_ACCOUNT,
            &encoded,
        ) {
            Ok(()) => return Ok("keychain"),
            Err(e) => {
                log::warn!("nearxd: keychain settings write failed ({e}), falling back to file");
            }
        }
    }

    write_signing_settings_file(settings)?;
    Ok("file")
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
    if source != "manual" && source != "seeded" {
        return Err((
            "ERR_PARAMS",
            "source must be 'manual' or 'seeded'".to_string(),
        ));
    }
    let prefer_keychain = crate::broker::parse_bool(params, "prefer_keychain", true);

    let mut settings = load_signing_settings().0.unwrap_or_else(|| json!({}));
    if !settings.is_object() {
        settings = json!({});
    }
    let entry = StakingWatchlistEntry {
        network: network.clone(),
        account_id: account_id.clone(),
        added_at_ms: now_ms(),
        source,
    };
    upsert_staking_watchlist_entry(&mut settings, entry);
    let settings_store =
        persist_signing_settings(&settings, prefer_keychain).map_err(|e| ("ERR_PERSIST", e))?;

    let entries = staking_watchlist_for_network(&settings, &network);
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
    let prefer_keychain = crate::broker::parse_bool(params, "prefer_keychain", true);

    let mut settings = load_signing_settings().0.unwrap_or_else(|| json!({}));
    if !settings.is_object() {
        settings = json!({});
    }
    let removed = remove_staking_watchlist_entry(&mut settings, &network, &account_id);
    let settings_store =
        persist_signing_settings(&settings, prefer_keychain).map_err(|e| ("ERR_PERSIST", e))?;

    let entries = staking_watchlist_for_network(&settings, &network);
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
