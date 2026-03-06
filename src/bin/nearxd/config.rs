use near_primitives::types::AccountId;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn home_dir() -> Option<PathBuf> {
    env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h.trim()))
        .filter(|p| !p.as_os_str().is_empty())
}

pub(crate) fn expand_tilde_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from("~"));
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}

#[derive(Debug, Deserialize)]
pub(crate) struct NearCliConfigFile {
    #[serde(default)]
    pub credentials_home_dir: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct NearCliUsedAccount {
    pub account_id: String,
    #[serde(default)]
    pub used_as_signer: bool,
}

#[cfg(target_os = "macos")]
pub(crate) fn near_cli_config_path() -> Option<PathBuf> {
    let home = home_dir()?;
    Some(
        home.join("Library")
            .join("Application Support")
            .join("near-cli")
            .join("config.toml"),
    )
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn near_cli_config_path() -> Option<PathBuf> {
    None
}

pub(crate) fn near_credentials_home_dir() -> Option<PathBuf> {
    if let Some(config_path) = near_cli_config_path() {
        if let Ok(raw) = fs::read_to_string(config_path) {
            if let Ok(cfg) = toml::from_str::<NearCliConfigFile>(&raw) {
                if let Some(custom) = cfg.credentials_home_dir.as_deref() {
                    let resolved = expand_tilde_path(custom);
                    if !resolved.as_os_str().is_empty() {
                        return Some(resolved);
                    }
                }
            }
        }
    }

    let home = home_dir()?;
    Some(home.join(".near-credentials"))
}

pub(crate) fn near_credentials_dir(network: &str) -> Option<PathBuf> {
    Some(near_credentials_home_dir()?.join(network))
}

pub(crate) fn load_near_cli_used_accounts(credentials_home_dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let path = credentials_home_dir.join("accounts.json");
    let Ok(raw) = fs::read_to_string(path) else {
        return out;
    };
    let Ok(accounts) = serde_json::from_str::<Vec<NearCliUsedAccount>>(&raw) else {
        return out;
    };
    for item in accounts {
        let account_id = item.account_id.trim();
        if account_id.is_empty() {
            continue;
        }
        // Keep only signer-capable history entries for account discovery parity with near-cli.
        if item.used_as_signer {
            out.insert(account_id.to_string());
        }
    }
    out
}

pub(crate) fn signing_settings_file_path() -> Option<PathBuf> {
    let home = home_dir()?;
    Some(home.join(".nearx").join("signing_settings.json"))
}

pub(crate) fn runtime_near_node_url() -> String {
    env::var("NEAR_NODE_URL").unwrap_or_else(|_| "https://rpc.mainnet.fastnear.com/".to_string())
}

pub(crate) fn runtime_fastnear_api_url() -> String {
    env::var("FASTNEAR_API_URL").unwrap_or_else(|_| "https://tx.main.fastnear.com".to_string())
}

pub(crate) fn validate_account_id_param(raw: &str) -> Result<String, String> {
    let normalized = raw.trim().to_string();
    if normalized.is_empty() {
        return Err("account_id cannot be empty".to_string());
    }
    let _: AccountId = normalized
        .parse()
        .map_err(|e| format!("invalid account_id: {e}"))?;
    Ok(normalized)
}
