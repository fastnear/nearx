use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::config::{expand_tilde_path, load_near_cli_used_accounts, near_credentials_home_dir};
use crate::credentials::{
    collect_legacy_credentials, credential_curve_type, near_cli_secure_has_credential,
    near_cli_secure_public_keys_for_account, nearxd_keychain_has_credential, normalize_source,
    normalize_sources, parse_credential_from_value, read_near_cli_secure_credential,
    read_near_credential_keychain, store_near_credential_keychain, ParsedNearCredential,
    DEFAULT_KEYCHAIN_CREDENTIAL_PROTECTION, SOURCE_HARDWARE_WALLET, SOURCE_LEGACY_FILE,
    SOURCE_NEARXD_KEYCHAIN, SOURCE_NEAR_CLI_SECURE,
};
use crate::rpc::{
    access_key_permission_to_summary, fetch_onchain_access_keys, is_full_access_permission,
};
use crate::settings::{
    indexed_signing_keys_for_network, is_index_record_stale, load_signing_settings,
    persist_signing_settings, upsert_signing_key_index, IndexedSigningKeyRecord,
};
use crate::user_presence::request_user_presence;
use crate::util::now_ms;

#[derive(Debug, Clone)]
pub(crate) struct DiscoveredSigningKey {
    pub account_id: String,
    pub public_key: String,
    pub permission: Value,
    pub available_sources: BTreeSet<String>,
    pub in_nearxd_keychain: bool,
    pub last_seen_at_ms: Option<u64>,
    pub stale: bool,
}

pub(crate) fn ordered_sources_from_set(sources: &BTreeSet<String>) -> Vec<String> {
    let mut out = Vec::new();
    for source in [
        SOURCE_NEARXD_KEYCHAIN,
        SOURCE_NEAR_CLI_SECURE,
        SOURCE_LEGACY_FILE,
        SOURCE_HARDWARE_WALLET,
    ] {
        if sources.contains(source) {
            out.push(source.to_string());
        }
    }
    out
}

pub(crate) fn preferred_source_from_set(sources: &BTreeSet<String>) -> Option<String> {
    for source in [
        SOURCE_NEARXD_KEYCHAIN,
        SOURCE_NEAR_CLI_SECURE,
        SOURCE_LEGACY_FILE,
        SOURCE_HARDWARE_WALLET,
    ] {
        if sources.contains(source) {
            return Some(source.to_string());
        }
    }
    None
}

fn parse_source_filters(params: &Value) -> Vec<String> {
    let mut sources = Vec::new();
    if let Some(one) = crate::broker::parse_string(params, "source") {
        if let Some(normalized) = normalize_source(one) {
            sources.push(normalized.to_string());
        }
    }
    if let Some(many) = params.get("sources").and_then(Value::as_array) {
        for v in many {
            if let Some(raw) = v.as_str() {
                if let Some(normalized) = normalize_source(raw) {
                    if !sources.iter().any(|s| s == normalized) {
                        sources.push(normalized.to_string());
                    }
                }
            }
        }
    }
    if sources.is_empty() {
        vec![
            SOURCE_LEGACY_FILE.to_string(),
            SOURCE_NEAR_CLI_SECURE.to_string(),
        ]
    } else {
        sources
    }
}

pub(crate) fn resolve_credentials_home_dir(params: &Value) -> Result<PathBuf, String> {
    if let Some(raw) = crate::broker::parse_string(params, "credentials_home_dir") {
        return Ok(expand_tilde_path(raw));
    }
    near_credentials_home_dir().ok_or_else(|| "unable to resolve credentials_home_dir".to_string())
}

pub(crate) fn discover_signing_accounts(
    network: &str,
    credentials_home_dir: &std::path::Path,
    settings: Option<&Value>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut accounts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    let legacy_dir = credentials_home_dir.join(network);
    if legacy_dir.exists() {
        if let Ok(legacy) = collect_legacy_credentials(&legacy_dir) {
            for item in legacy {
                accounts
                    .entry(item.credential.account_id)
                    .or_default()
                    .insert(SOURCE_LEGACY_FILE.to_string());
            }
        }
    }

    for account_id in load_near_cli_used_accounts(credentials_home_dir) {
        accounts
            .entry(account_id)
            .or_default()
            .insert(SOURCE_NEAR_CLI_SECURE.to_string());
    }

    if let Some(settings) = settings {
        for indexed in indexed_signing_keys_for_network(settings, network) {
            let bucket = accounts.entry(indexed.account_id.clone()).or_default();
            for source in &indexed.available_sources {
                if let Some(normalized) = normalize_source(source) {
                    bucket.insert(normalized.to_string());
                }
            }
            if indexed.in_nearxd_keychain {
                bucket.insert(SOURCE_NEARXD_KEYCHAIN.to_string());
            }
        }
    }

    accounts
}

pub(crate) fn build_signing_keys(
    network: &str,
    account_filter: Option<&str>,
    credentials_home_dir: &std::path::Path,
    settings: Option<&Value>,
) -> Vec<DiscoveredSigningKey> {
    let now = now_ms();
    let legacy_dir = credentials_home_dir.join(network);
    let legacy_credentials = if legacy_dir.exists() {
        collect_legacy_credentials(&legacy_dir).unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut legacy_by_key: BTreeMap<(String, String), crate::credentials::ParsedLegacyCredential> =
        BTreeMap::new();
    for item in legacy_credentials {
        legacy_by_key.insert(
            (
                item.credential.account_id.clone(),
                item.credential.public_key.clone(),
            ),
            item,
        );
    }

    let indexed = settings
        .map(|s| indexed_signing_keys_for_network(s, network))
        .unwrap_or_default();
    let mut indexed_by_key: BTreeMap<(String, String), IndexedSigningKeyRecord> = BTreeMap::new();
    for mut rec in indexed {
        let key = (rec.account_id.clone(), rec.public_key.clone());
        rec.available_sources = normalize_sources(&rec.available_sources);
        if let Some(prev) = indexed_by_key.get_mut(&key) {
            for source in &rec.available_sources {
                if !prev.available_sources.iter().any(|s| s == source) {
                    prev.available_sources.push(source.clone());
                }
            }
            prev.in_nearxd_keychain |= rec.in_nearxd_keychain;
            prev.last_seen_at_ms = prev.last_seen_at_ms.max(rec.last_seen_at_ms);
        } else {
            indexed_by_key.insert(key, rec);
        }
    }

    let discovered_accounts = discover_signing_accounts(network, credentials_home_dir, settings);
    let mut account_ids: BTreeSet<String> = discovered_accounts.keys().cloned().collect();
    if let Some(account) = account_filter {
        account_ids.clear();
        if !account.trim().is_empty() {
            account_ids.insert(account.to_string());
        }
    }

    let mut out = Vec::new();
    for account_id in account_ids {
        let mut by_key: BTreeMap<String, DiscoveredSigningKey> = BTreeMap::new();

        match fetch_onchain_access_keys(&account_id) {
            Ok(onchain_keys) => {
                for item in onchain_keys {
                    let public_key = item.public_key.trim().to_string();
                    if public_key.is_empty() {
                        continue;
                    }
                    let permission = access_key_permission_to_summary(&item.access_key);
                    let mut available_sources = BTreeSet::new();
                    if nearxd_keychain_has_credential(network, &account_id, &public_key) {
                        available_sources.insert(SOURCE_NEARXD_KEYCHAIN.to_string());
                    }
                    if near_cli_secure_has_credential(network, &account_id, &public_key) {
                        available_sources.insert(SOURCE_NEAR_CLI_SECURE.to_string());
                    }
                    if legacy_by_key.contains_key(&(account_id.clone(), public_key.clone())) {
                        available_sources.insert(SOURCE_LEGACY_FILE.to_string());
                    }
                    if let Some(indexed) =
                        indexed_by_key.get(&(account_id.clone(), public_key.clone()))
                    {
                        for source in &indexed.available_sources {
                            if let Some(normalized) = normalize_source(source) {
                                available_sources.insert(normalized.to_string());
                            }
                        }
                        if indexed.in_nearxd_keychain {
                            available_sources.insert(SOURCE_NEARXD_KEYCHAIN.to_string());
                        }
                    }
                    let in_nearxd_keychain = available_sources.contains(SOURCE_NEARXD_KEYCHAIN);
                    let indexed_last_seen = indexed_by_key
                        .get(&(account_id.clone(), public_key.clone()))
                        .and_then(|r| (r.last_seen_at_ms > 0).then_some(r.last_seen_at_ms));
                    by_key.insert(
                        public_key.clone(),
                        DiscoveredSigningKey {
                            account_id: account_id.clone(),
                            public_key,
                            permission,
                            available_sources,
                            in_nearxd_keychain,
                            last_seen_at_ms: indexed_last_seen,
                            stale: false,
                        },
                    );
                }
            }
            Err(e) => {
                log::warn!(
                    "nearxd: failed to fetch on-chain access keys for {} on {}: {}",
                    account_id,
                    network,
                    e
                );
                let near_cli_fallback =
                    near_cli_secure_public_keys_for_account(network, &account_id);
                if !near_cli_fallback.is_empty() {
                    log::info!(
                        "nearxd: recovered {} key(s) for {} from near-cli secure keychain fallback",
                        near_cli_fallback.len(),
                        account_id
                    );
                }
                for public_key in near_cli_fallback {
                    let mut available_sources = BTreeSet::new();
                    available_sources.insert(SOURCE_NEAR_CLI_SECURE.to_string());
                    if nearxd_keychain_has_credential(network, &account_id, &public_key) {
                        available_sources.insert(SOURCE_NEARXD_KEYCHAIN.to_string());
                    }
                    if legacy_by_key.contains_key(&(account_id.clone(), public_key.clone())) {
                        available_sources.insert(SOURCE_LEGACY_FILE.to_string());
                    }
                    if let Some(indexed) =
                        indexed_by_key.get(&(account_id.clone(), public_key.clone()))
                    {
                        for source in &indexed.available_sources {
                            if let Some(normalized) = normalize_source(source) {
                                available_sources.insert(normalized.to_string());
                            }
                        }
                        if indexed.in_nearxd_keychain {
                            available_sources.insert(SOURCE_NEARXD_KEYCHAIN.to_string());
                        }
                    }
                    let in_nearxd_keychain = available_sources.contains(SOURCE_NEARXD_KEYCHAIN);
                    by_key
                        .entry(public_key.clone())
                        .or_insert(DiscoveredSigningKey {
                            account_id: account_id.clone(),
                            public_key: public_key.clone(),
                            permission: json!({ "kind": "unknown" }),
                            available_sources,
                            in_nearxd_keychain,
                            last_seen_at_ms: indexed_by_key
                                .get(&(account_id.clone(), public_key.clone()))
                                .and_then(|r| (r.last_seen_at_ms > 0).then_some(r.last_seen_at_ms)),
                            stale: false,
                        });
                }
            }
        }

        for ((aid, public_key), item) in &legacy_by_key {
            if aid != &account_id {
                continue;
            }
            by_key.entry(public_key.clone()).or_insert_with(|| {
                let mut available_sources = BTreeSet::new();
                available_sources.insert(SOURCE_LEGACY_FILE.to_string());
                if nearxd_keychain_has_credential(network, &item.credential.account_id, public_key)
                {
                    available_sources.insert(SOURCE_NEARXD_KEYCHAIN.to_string());
                }
                DiscoveredSigningKey {
                    account_id: account_id.clone(),
                    public_key: public_key.clone(),
                    permission: json!({ "kind": "unknown" }),
                    in_nearxd_keychain: available_sources.contains(SOURCE_NEARXD_KEYCHAIN),
                    available_sources,
                    last_seen_at_ms: indexed_by_key
                        .get(&(account_id.clone(), public_key.clone()))
                        .and_then(|r| (r.last_seen_at_ms > 0).then_some(r.last_seen_at_ms)),
                    stale: false,
                }
            });
        }

        for ((aid, public_key), indexed) in &indexed_by_key {
            if aid != &account_id {
                continue;
            }
            by_key
                .entry(public_key.clone())
                .and_modify(|entry| {
                    for source in &indexed.available_sources {
                        if let Some(normalized) = normalize_source(source) {
                            entry.available_sources.insert(normalized.to_string());
                        }
                    }
                    if indexed.in_nearxd_keychain {
                        entry
                            .available_sources
                            .insert(SOURCE_NEARXD_KEYCHAIN.to_string());
                    }
                    entry.in_nearxd_keychain =
                        entry.available_sources.contains(SOURCE_NEARXD_KEYCHAIN);
                    if indexed.last_seen_at_ms > 0 {
                        entry.last_seen_at_ms = Some(
                            entry
                                .last_seen_at_ms
                                .unwrap_or(0)
                                .max(indexed.last_seen_at_ms),
                        );
                    }
                })
                .or_insert_with(|| {
                    let mut available_sources = BTreeSet::new();
                    for source in &indexed.available_sources {
                        if let Some(normalized) = normalize_source(source) {
                            available_sources.insert(normalized.to_string());
                        }
                    }
                    if indexed.in_nearxd_keychain {
                        available_sources.insert(SOURCE_NEARXD_KEYCHAIN.to_string());
                    }
                    let last_seen_at_ms =
                        (indexed.last_seen_at_ms > 0).then_some(indexed.last_seen_at_ms);
                    DiscoveredSigningKey {
                        account_id: account_id.clone(),
                        public_key: public_key.clone(),
                        permission: json!({ "kind": "unknown" }),
                        in_nearxd_keychain: available_sources.contains(SOURCE_NEARXD_KEYCHAIN),
                        available_sources,
                        last_seen_at_ms,
                        stale: is_index_record_stale(indexed.last_seen_at_ms, now),
                    }
                });
        }

        out.extend(by_key.into_values());
    }

    out
}

pub(crate) fn resolve_signing_credential(
    network: &str,
    account_id: &str,
    signer_public_key: Option<&str>,
    credential_source: Option<&str>,
    reason: &str,
) -> Result<(Value, String), String> {
    let mut source_order = Vec::new();
    if let Some(source) = credential_source.and_then(normalize_source) {
        source_order.push(source.to_string());
    } else {
        source_order.push(SOURCE_NEARXD_KEYCHAIN.to_string());
        source_order.push(SOURCE_NEAR_CLI_SECURE.to_string());
    }

    let mut candidates: Vec<(String, bool)> = Vec::new();
    if let Some(public_key) = signer_public_key {
        candidates.push((public_key.to_string(), true));
    } else {
        match fetch_onchain_access_keys(account_id) {
            Ok(keys) => {
                for key in keys {
                    let public_key = key.public_key.trim().to_string();
                    if public_key.is_empty() {
                        continue;
                    }
                    let permission = access_key_permission_to_summary(&key.access_key);
                    candidates.push((public_key, is_full_access_permission(&permission)));
                }
            }
            Err(e) => {
                log::warn!(
                    "nearxd: failed to fetch on-chain keys for signer {} on {}: {}",
                    account_id,
                    network,
                    e
                );
                let fallback = near_cli_secure_public_keys_for_account(network, account_id);
                if !fallback.is_empty() {
                    log::info!(
                        "nearxd: using near-cli secure keychain fallback for signer {} ({} key(s))",
                        account_id,
                        fallback.len()
                    );
                }
                for public_key in fallback {
                    candidates.push((public_key, false));
                }
            }
        }
        candidates.sort_by(|a, b| b.1.cmp(&a.1));
        candidates.dedup_by(|a, b| a.0 == b.0);
    }

    let mut errors = Vec::new();
    for source in source_order {
        match source.as_str() {
            SOURCE_NEARXD_KEYCHAIN => {
                if signer_public_key.is_none() {
                    match read_near_credential_keychain(network, account_id, None, reason) {
                        Ok(payload) => return Ok((payload, SOURCE_NEARXD_KEYCHAIN.to_string())),
                        Err(e) => errors.push(format!("nearxd_keychain (legacy namespace): {e}")),
                    }
                }
                for (public_key, _) in &candidates {
                    if !nearxd_keychain_has_credential(network, account_id, public_key) {
                        continue;
                    }
                    match read_near_credential_keychain(
                        network,
                        account_id,
                        Some(public_key),
                        reason,
                    ) {
                        Ok(payload) => return Ok((payload, SOURCE_NEARXD_KEYCHAIN.to_string())),
                        Err(e) => errors.push(format!("nearxd_keychain {}: {}", public_key, e)),
                    }
                }
            }
            SOURCE_NEAR_CLI_SECURE => {
                for (public_key, _) in &candidates {
                    if !near_cli_secure_has_credential(network, account_id, public_key) {
                        continue;
                    }
                    match read_near_cli_secure_credential(network, account_id, public_key) {
                        Ok(payload) => return Ok((payload, SOURCE_NEAR_CLI_SECURE.to_string())),
                        Err(e) => errors.push(format!("near_cli_secure {}: {}", public_key, e)),
                    }
                }
                if signer_public_key.is_none() && candidates.is_empty() {
                    errors.push(
                        "near_cli_secure: no signer_public_key and no on-chain keys available"
                            .to_string(),
                    );
                }
            }
            _ => {}
        }
    }

    if errors.is_empty() {
        Err(
            "no credential source could provide a signing key; import from legacy or near-cli secure storage first"
                .to_string(),
        )
    } else {
        Err(errors.join("; "))
    }
}

// Result handlers for signing-related broker methods

pub(crate) fn list_near_signing_accounts_result(
    params: &Value,
) -> Result<Value, (&'static str, String)> {
    let network =
        crate::broker::resolve_network_param(params, "mainnet").map_err(|e| ("ERR_PARAMS", e))?;
    let credentials_home_dir = resolve_credentials_home_dir(params).map_err(|e| ("ERR_IO", e))?;
    let (settings, settings_source) = load_signing_settings();

    let accounts_map =
        discover_signing_accounts(&network, &credentials_home_dir, settings.as_ref());
    let mut accounts = Vec::new();
    for (account_id, source_hints_set) in accounts_map {
        let source_hints = ordered_sources_from_set(&source_hints_set);
        let has_keys = source_hints_set.contains(SOURCE_LEGACY_FILE)
            || source_hints_set.contains(SOURCE_NEARXD_KEYCHAIN)
            || source_hints_set.contains(SOURCE_HARDWARE_WALLET);
        accounts.push(json!({
            "account_id": account_id,
            "has_keys": has_keys,
            "source_hints": source_hints,
        }));
    }

    Ok(json!({
        "network": network,
        "credentials_home_dir": credentials_home_dir.display().to_string(),
        "accounts": accounts,
        "settings_source": settings_source,
    }))
}

pub(crate) fn list_near_signing_keys_result(
    params: &Value,
) -> Result<Value, (&'static str, String)> {
    let network =
        crate::broker::resolve_network_param(params, "mainnet").map_err(|e| ("ERR_PARAMS", e))?;
    let credentials_home_dir = resolve_credentials_home_dir(params).map_err(|e| ("ERR_IO", e))?;
    let account_filter = crate::broker::parse_string(params, "account_id");

    let (settings, settings_source) = load_signing_settings();
    let keys = build_signing_keys(
        &network,
        account_filter,
        &credentials_home_dir,
        settings.as_ref(),
    );

    let mut out_keys = Vec::new();
    for key in keys {
        let sources = ordered_sources_from_set(&key.available_sources);
        let preferred_source = preferred_source_from_set(&key.available_sources);
        let importable = !key.in_nearxd_keychain
            && (key.available_sources.contains(SOURCE_LEGACY_FILE)
                || key.available_sources.contains(SOURCE_NEAR_CLI_SECURE));
        out_keys.push(json!({
            "account_id": key.account_id,
            "public_key": key.public_key,
            "curve_type": credential_curve_type(&key.public_key),
            "permission": key.permission,
            "available_sources": sources,
            "preferred_source": preferred_source,
            "in_nearxd_keychain": key.in_nearxd_keychain,
            "importable": importable,
            "last_seen_at_ms": key.last_seen_at_ms,
            "stale": key.stale,
        }));
    }

    Ok(json!({
        "network": network,
        "credentials_home_dir": credentials_home_dir.display().to_string(),
        "keys": out_keys,
        "settings_source": settings_source,
    }))
}

pub(crate) fn import_near_signing_keys_result(
    params: &Value,
    forced_sources: Option<Vec<String>>,
) -> Result<Value, (&'static str, String)> {
    let network =
        crate::broker::resolve_network_param(params, "testnet").map_err(|e| ("ERR_PARAMS", e))?;
    let credentials_home_dir = resolve_credentials_home_dir(params).map_err(|e| ("ERR_IO", e))?;
    let credentials_dir = if let Some(raw) = crate::broker::parse_string(params, "credentials_dir")
    {
        expand_tilde_path(raw)
    } else {
        credentials_home_dir.join(&network)
    };

    let account_filter = crate::broker::parse_string(params, "account_id").map(str::to_string);
    let public_key_filter = crate::broker::parse_string(params, "public_key").map(str::to_string);
    let keychain_protection = crate::broker::parse_string(params, "keychain_credential_protection")
        .unwrap_or(DEFAULT_KEYCHAIN_CREDENTIAL_PROTECTION)
        .trim()
        .to_ascii_lowercase();
    if keychain_protection != "biometry_current_set" && keychain_protection != "user_presence" {
        return Err((
            "ERR_PARAMS",
            "keychain_credential_protection must be 'biometry_current_set' or 'user_presence'"
                .to_string(),
        ));
    }

    let require_user_presence =
        crate::broker::parse_bool(params, "require_user_presence", cfg!(target_os = "macos"));
    let allow_fallback_default = keychain_protection != "biometry_current_set";
    let allow_fallback = crate::broker::parse_optional_bool(params, "allow_fallback")
        .unwrap_or(allow_fallback_default);
    let reason = crate::broker::parse_string(params, "reason")
        .unwrap_or("NEARx needs your approval to import NEAR credentials.");
    let persist_in_keychain =
        crate::broker::parse_bool(params, "persist_in_keychain", cfg!(target_os = "macos"));
    let overwrite = crate::broker::parse_bool(params, "overwrite", false);
    let save_settings = crate::broker::parse_bool(params, "save_settings", true);
    let max_keys = crate::broker::parse_u64(
        params,
        "max_keys",
        crate::broker::parse_u64(params, "max_accounts", 200),
    )
    .clamp(1, 2_000) as usize;

    if persist_in_keychain && !cfg!(target_os = "macos") {
        return Err((
            "ERR_UNAVAILABLE",
            "persist_in_keychain is only supported on macOS".to_string(),
        ));
    }

    let sources = forced_sources.unwrap_or_else(|| parse_source_filters(params));
    let mut source_set = BTreeSet::new();
    for source in sources {
        if let Some(normalized) = normalize_source(&source) {
            source_set.insert(normalized.to_string());
        }
    }
    if source_set.is_empty() {
        source_set.insert(SOURCE_LEGACY_FILE.to_string());
    }

    let user_presence = if require_user_presence {
        request_user_presence(reason, allow_fallback).map_err(|e| ("ERR_AUTH", e))?
    } else {
        json!({
            "verified": false,
            "skipped": true,
            "reason": "require_user_presence=false"
        })
    };

    #[derive(Debug, Clone)]
    struct ImportCandidate {
        credential: ParsedNearCredential,
        source: String,
        file: Option<String>,
    }

    let mut candidates: BTreeMap<(String, String), ImportCandidate> = BTreeMap::new();

    if source_set.contains(SOURCE_LEGACY_FILE) && credentials_dir.exists() {
        let legacy = collect_legacy_credentials(&credentials_dir).map_err(|e| ("ERR_IO", e))?;
        for item in legacy {
            if let Some(filter) = account_filter.as_deref() {
                if item.credential.account_id != filter {
                    continue;
                }
            }
            if let Some(filter) = public_key_filter.as_deref() {
                if item.credential.public_key != filter {
                    continue;
                }
            }
            let key = (
                item.credential.account_id.clone(),
                item.credential.public_key.clone(),
            );
            candidates.entry(key).or_insert(ImportCandidate {
                credential: item.credential,
                source: SOURCE_LEGACY_FILE.to_string(),
                file: Some(item.path.display().to_string()),
            });
        }
    }

    if source_set.contains(SOURCE_NEAR_CLI_SECURE) {
        let mut account_ids: Vec<String> = if let Some(account) = account_filter.as_deref() {
            vec![account.to_string()]
        } else {
            load_near_cli_used_accounts(&credentials_home_dir)
                .into_iter()
                .collect()
        };
        account_ids.sort();
        account_ids.dedup();

        for account_id in account_ids {
            let mut public_keys = Vec::new();
            if let Some(pk) = public_key_filter.as_deref() {
                public_keys.push(pk.to_string());
            } else {
                match fetch_onchain_access_keys(&account_id) {
                    Ok(keys) => {
                        for key in keys {
                            let pk = key.public_key.trim();
                            if !pk.is_empty() {
                                public_keys.push(pk.to_string());
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "nearxd: rpc key discovery failed for {} during near-cli secure import: {}",
                            account_id,
                            e
                        );
                        let fallback =
                            near_cli_secure_public_keys_for_account(&network, &account_id);
                        if !fallback.is_empty() {
                            log::info!(
                                "nearxd: using near-cli secure keychain fallback for {} ({} key(s))",
                                account_id,
                                fallback.len()
                            );
                            public_keys.extend(fallback);
                        }
                    }
                }
            }
            public_keys.sort();
            public_keys.dedup();

            for public_key in public_keys {
                if !near_cli_secure_has_credential(&network, &account_id, &public_key) {
                    continue;
                }
                let value =
                    match read_near_cli_secure_credential(&network, &account_id, &public_key) {
                        Ok(v) => v,
                        Err(e) => {
                            log::warn!(
                                "nearxd: near-cli secure read failed for {}:{}:{}: {}",
                                network,
                                account_id,
                                public_key,
                                e
                            );
                            continue;
                        }
                    };
                let credential = match parse_credential_from_value(&value) {
                    Ok(v) => v,
                    Err(e) => {
                        log::warn!(
                            "nearxd: near-cli secure payload parse failed for {}:{}:{}: {}",
                            network,
                            account_id,
                            public_key,
                            e
                        );
                        continue;
                    }
                };
                let key = (credential.account_id.clone(), credential.public_key.clone());
                candidates.entry(key).or_insert(ImportCandidate {
                    credential,
                    source: SOURCE_NEAR_CLI_SECURE.to_string(),
                    file: None,
                });
            }
        }
    }

    let mut imported = Vec::<Value>::new();
    let mut skipped = Vec::<Value>::new();
    let mut failed = Vec::<Value>::new();
    let mut index_updates = Vec::<IndexedSigningKeyRecord>::new();

    for candidate in candidates.into_values().take(max_keys) {
        let keychain_account = crate::credentials::nearxd_credential_account_key(
            &network,
            &candidate.credential.account_id,
            &candidate.credential.public_key,
        );
        let mut keychain_status = "not_requested".to_string();
        if persist_in_keychain {
            match store_near_credential_keychain(
                &network,
                &candidate.credential,
                overwrite,
                &keychain_protection,
            ) {
                Ok(status) => keychain_status = status.to_string(),
                Err(e) => {
                    failed.push(json!({
                        "account_id": candidate.credential.account_id,
                        "public_key": candidate.credential.public_key,
                        "source": candidate.source,
                        "file": candidate.file,
                        "error": e,
                    }));
                    continue;
                }
            }
        }

        let mut sources = vec![candidate.source.clone()];
        let in_nearxd_keychain = persist_in_keychain && keychain_status.starts_with("stored")
            || keychain_status == "skipped_existing";
        if in_nearxd_keychain {
            sources.push(SOURCE_NEARXD_KEYCHAIN.to_string());
        }
        index_updates.push(IndexedSigningKeyRecord {
            network: network.clone(),
            account_id: candidate.credential.account_id.clone(),
            public_key: candidate.credential.public_key.clone(),
            available_sources: sources,
            in_nearxd_keychain,
            last_seen_at_ms: now_ms(),
        });

        let row = json!({
            "account_id": candidate.credential.account_id,
            "public_key": candidate.credential.public_key,
            "curve_type": credential_curve_type(&candidate.credential.public_key),
            "source": candidate.source,
            "file": candidate.file,
            "keychain_account": if persist_in_keychain { Some(keychain_account) } else { None::<String> },
            "keychain_status": keychain_status,
        });
        if row
            .get("keychain_status")
            .and_then(Value::as_str)
            .map(|s| s == "skipped_existing")
            .unwrap_or(false)
        {
            skipped.push(row);
        } else {
            imported.push(row);
        }
    }

    let mut settings_save = json!({
        "saved": false,
        "source": "none",
    });
    if save_settings {
        let mut settings = load_signing_settings().0.unwrap_or_else(|| json!({}));
        if !settings.is_object() {
            settings = json!({});
        }
        if let Some(map) = settings.as_object_mut() {
            map.insert(
                "near_credentials".to_string(),
                json!({
                    "network": network,
                    "credentials_dir": credentials_dir.display().to_string(),
                    "credentials_home_dir": credentials_home_dir.display().to_string(),
                    "last_imported_at_ms": now_ms(),
                    "account_filter": account_filter,
                    "public_key_filter": public_key_filter,
                    "imported_accounts": imported
                        .iter()
                        .filter_map(|v| v.get("account_id").and_then(Value::as_str))
                        .collect::<Vec<_>>(),
                    "require_user_presence": require_user_presence,
                    "persist_in_keychain": persist_in_keychain,
                    "keychain_credential_protection": keychain_protection,
                    "sources": source_set.iter().cloned().collect::<Vec<_>>(),
                }),
            );
        }
        upsert_signing_key_index(&mut settings, &index_updates);

        match persist_signing_settings(&settings, true) {
            Ok(source) => {
                settings_save = json!({
                    "saved": true,
                    "source": source,
                });
            }
            Err(e) => {
                settings_save = json!({
                    "saved": false,
                    "source": "none",
                    "error": e,
                });
            }
        }
    }

    Ok(json!({
        "network": network,
        "credentials_home_dir": credentials_home_dir.display().to_string(),
        "credentials_dir": credentials_dir.display().to_string(),
        "imported_count": imported.len(),
        "imported": imported,
        "skipped": skipped,
        "failed": failed,
        "user_presence": user_presence,
        "settings_save": settings_save,
        "keychain_credential_protection": if persist_in_keychain { Some(keychain_protection) } else { None::<String> },
        "sources": source_set.into_iter().collect::<Vec<_>>(),
    }))
}
