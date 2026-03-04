//! nearxd - local broker daemon for NEARx desktop integrations.
//!
//! This process provides a small local JSON API over a Unix socket:
//! - central deep-link validation/opening
//! - runtime config discovery
//! - FastNEAR token resolution with persistence fallback
//!
//! Protocol:
//!   request:  {"id":"1","method":"ping","params":{}}
//!   response: {"id":"1","ok":true,"result":{"..."}}

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD}, Engine as _};
use near_crypto::{PublicKey, SecretKey};
use near_primitives::action::{Action, FunctionCallAction, TransferAction};
use near_primitives::hash::CryptoHash;
use near_primitives::transaction::{SignedTransaction, Transaction, TransactionV0};
use near_primitives::types::AccountId;
use rand::RngCore;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};

const BROKER_VERSION: u8 = 1;
const DEFAULT_INTENT_TTL_MS: u64 = 2 * 60 * 1000; // 2 minutes
const MAX_INTENT_TTL_MS: u64 = 10 * 60 * 1000; // 10 minutes
const KEYCHAIN_SERVICE: &str = "nearxd.fastnear.auth";
const KEYCHAIN_ACCOUNT: &str = "fastnear_auth_token";
const KEYCHAIN_NEAR_CREDENTIAL_SERVICE: &str = "nearxd.near.credentials";
const KEYCHAIN_SIGNING_SETTINGS_SERVICE: &str = "nearxd.signing.settings";
const KEYCHAIN_SIGNING_SETTINGS_ACCOUNT: &str = "default";
const DEFAULT_USER_PRESENCE_REASON: &str = "NEARx needs your approval to continue.";
const DEFAULT_KEYCHAIN_CREDENTIAL_PROTECTION: &str = "biometry_current_set";

#[derive(Debug)]
struct BrokerState {
    session_token: Mutex<Option<String>>,
    token_store: Arc<dyn TokenStore>,
    sign_intents: Mutex<std::collections::HashMap<String, SignIntent>>,
}

impl BrokerState {
    fn new() -> Self {
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
struct SignIntent {
    account_id: String,
    payload: Value,
    origin: Option<String>,
    challenge: String,
    created_at_ms: u64,
    expires_at_ms: u64,
    approved: bool,
    require_user_presence: bool,
    user_presence_reason: Option<String>,
    user_presence_verified: bool,
    user_presence_modality: Option<String>,
}

trait TokenStore: Send + Sync + std::fmt::Debug {
    fn backend_name(&self) -> &'static str;
    fn read_token(&self) -> Option<String>;
    fn persist_token(&self, token: &str) -> Result<(), String>;
    fn clear_token(&self) -> Result<(), String>;
}

#[derive(Debug)]
struct FileTokenStore;

impl TokenStore for FileTokenStore {
    fn backend_name(&self) -> &'static str {
        "file"
    }

    fn read_token(&self) -> Option<String> {
        read_file_token()
    }

    fn persist_token(&self, token: &str) -> Result<(), String> {
        persist_file_token(token)
    }

    fn clear_token(&self) -> Result<(), String> {
        clear_file_token()
    }
}

#[derive(Debug)]
struct MacKeychainTokenStore;

impl TokenStore for MacKeychainTokenStore {
    fn backend_name(&self) -> &'static str {
        "keychain"
    }

    fn read_token(&self) -> Option<String> {
        read_keychain_token()
    }

    fn persist_token(&self, token: &str) -> Result<(), String> {
        persist_keychain_token(token)
    }

    fn clear_token(&self) -> Result<(), String> {
        clear_keychain_token()
    }
}

#[derive(Debug)]
struct AutoTokenStore {
    primary: Arc<dyn TokenStore>,
    fallback: Arc<dyn TokenStore>,
}

impl TokenStore for AutoTokenStore {
    fn backend_name(&self) -> &'static str {
        self.primary.backend_name()
    }

    fn read_token(&self) -> Option<String> {
        self.primary
            .read_token()
            .or_else(|| self.fallback.read_token())
    }

    fn persist_token(&self, token: &str) -> Result<(), String> {
        match self.primary.persist_token(token) {
            Ok(()) => Ok(()),
            Err(e) => {
                log::warn!(
                    "nearxd: token store '{}' failed ({}), falling back to '{}'",
                    self.primary.backend_name(),
                    e,
                    self.fallback.backend_name()
                );
                self.fallback.persist_token(token)
            }
        }
    }

    fn clear_token(&self) -> Result<(), String> {
        match self.primary.clear_token() {
            Ok(()) => Ok(()),
            Err(e) => {
                log::warn!(
                    "nearxd: clear token failed in '{}' ({}), falling back to '{}'",
                    self.primary.backend_name(),
                    e,
                    self.fallback.backend_name()
                );
                self.fallback.clear_token()
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct BrokerRequest {
    #[serde(default)]
    id: Option<String>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct BrokerResponse {
    id: String,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<BrokerError>,
}

#[derive(Debug, Serialize)]
struct BrokerError {
    code: &'static str,
    message: String,
}

impl BrokerResponse {
    fn ok(id: String, result: Value) -> Self {
        Self {
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    fn err(id: String, code: &'static str, message: impl Into<String>) -> Self {
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

fn id_or_default(id: Option<String>) -> String {
    id.unwrap_or_else(|| "0".to_string())
}

fn parse_bool(params: &Value, key: &str, default: bool) -> bool {
    params.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn parse_string<'a>(params: &'a Value, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str).map(str::trim)
}

fn parse_u64(params: &Value, key: &str, default: u64) -> u64 {
    params.get(key).and_then(Value::as_u64).unwrap_or(default)
}

fn parse_optional_bool(params: &Value, key: &str) -> Option<bool> {
    params.get(key).and_then(Value::as_bool)
}

fn parse_near_actions(actions_json: &[Value]) -> Result<Vec<Action>, String> {
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
                let deposit_str = a
                    .get("deposit")
                    .and_then(Value::as_str)
                    .unwrap_or("0");
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

fn home_dir() -> Option<PathBuf> {
    env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h.trim()))
        .filter(|p| !p.as_os_str().is_empty())
}

fn expand_tilde_path(raw: &str) -> PathBuf {
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

fn near_credentials_dir(network: &str) -> Option<PathBuf> {
    let home = home_dir()?;
    Some(home.join(".near-credentials").join(network))
}

fn signing_settings_file_path() -> Option<PathBuf> {
    let home = home_dir()?;
    Some(home.join(".nearx").join("signing_settings.json"))
}

fn runtime_near_node_url() -> String {
    env::var("NEAR_NODE_URL").unwrap_or_else(|_| "https://rpc.mainnet.fastnear.com/".to_string())
}

fn runtime_fastnear_api_url() -> String {
    env::var("FASTNEAR_API_URL").unwrap_or_else(|_| "https://tx.main.fastnear.com".to_string())
}

fn build_token_store() -> Arc<dyn TokenStore> {
    match env::var("NEARXD_TOKEN_BACKEND")
        .unwrap_or_else(|_| "auto".to_string())
        .to_ascii_lowercase()
        .as_str()
    {
        "file" => Arc::new(FileTokenStore),
        "keychain" => {
            #[cfg(target_os = "macos")]
            {
                Arc::new(MacKeychainTokenStore)
            }
            #[cfg(not(target_os = "macos"))]
            {
                log::warn!(
                    "nearxd: keychain backend requested on non-macos host, using file backend"
                );
                Arc::new(FileTokenStore)
            }
        }
        _ => {
            #[cfg(target_os = "macos")]
            {
                Arc::new(AutoTokenStore {
                    primary: Arc::new(MacKeychainTokenStore),
                    fallback: Arc::new(FileTokenStore),
                })
            }
            #[cfg(not(target_os = "macos"))]
            {
                Arc::new(FileTokenStore)
            }
        }
    }
}

fn token_file_path() -> Option<PathBuf> {
    if let Ok(custom) = env::var("NEARXD_TOKEN_FILE") {
        let p = PathBuf::from(custom.trim());
        if !p.as_os_str().is_empty() {
            return Some(p);
        }
    }

    let home = env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join(".nearx")
            .join("fastnear_auth_token"),
    )
}

fn read_file_token() -> Option<String> {
    let path = token_file_path()?;
    let raw = fs::read_to_string(path).ok()?;
    let token = raw.trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn persist_file_token(token: &str) -> Result<(), String> {
    let Some(path) = token_file_path() else {
        return Err("NEARXD token path unavailable".to_string());
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create token directory: {e}"))?;
    }

    fs::write(&path, token.as_bytes()).map_err(|e| format!("write token file: {e}"))?;

    #[cfg(unix)]
    {
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms).map_err(|e| format!("chmod token file: {e}"))?;
    }

    Ok(())
}

fn clear_file_token() -> Result<(), String> {
    let Some(path) = token_file_path() else {
        return Ok(());
    };

    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("remove token file: {e}")),
    }
}

#[cfg(target_os = "macos")]
fn keychain_read_generic(service: &str, account: &str) -> Option<String> {
    use std::process::Command;

    let output = Command::new("security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let token = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

#[cfg(target_os = "macos")]
fn keychain_write_generic(service: &str, account: &str, value: &str) -> Result<(), String> {
    use std::process::Command;

    let status = Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            service,
            "-a",
            account,
            "-w",
            value,
        ])
        .status()
        .map_err(|e| format!("security add-generic-password failed: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err("security add-generic-password returned non-zero status".to_string())
    }
}

#[cfg(target_os = "macos")]
fn keychain_delete_generic(service: &str, account: &str) -> Result<(), String> {
    use std::process::Command;

    let status = Command::new("security")
        .args(["delete-generic-password", "-s", service, "-a", account])
        .status()
        .map_err(|e| format!("security delete-generic-password failed: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        // Treat missing item as cleared for idempotency.
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn keychain_has_generic(service: &str, account: &str) -> bool {
    let script = r#"
import Foundation
import Security

let service = CommandLine.arguments[1]
let account = CommandLine.arguments[2]

let query: [String: Any] = [
    kSecClass as String: kSecClassGenericPassword,
    kSecAttrService as String: service,
    kSecAttrAccount as String: account,
    kSecReturnAttributes as String: true,
    kSecMatchLimit as String: kSecMatchLimitOne
]

let status = SecItemCopyMatching(query as CFDictionary, nil)
if status == errSecSuccess {
    print("{\"found\":true}")
    exit(0)
}
print("{\"found\":false}")
exit(0)
"#;

    match run_swift_json(script, &[service, account]) {
        Ok(v) => v.get("found").and_then(Value::as_bool).unwrap_or(false),
        Err(_) => false,
    }
}

#[cfg(not(target_os = "macos"))]
fn keychain_read_generic(_service: &str, _account: &str) -> Option<String> {
    None
}

#[cfg(not(target_os = "macos"))]
fn keychain_write_generic(_service: &str, _account: &str, _value: &str) -> Result<(), String> {
    Err("keychain backend unavailable on this platform".to_string())
}

#[cfg(not(target_os = "macos"))]
fn keychain_delete_generic(_service: &str, _account: &str) -> Result<(), String> {
    Err("keychain backend unavailable on this platform".to_string())
}

#[cfg(not(target_os = "macos"))]
fn keychain_has_generic(_service: &str, _account: &str) -> bool {
    false
}

fn read_keychain_token() -> Option<String> {
    keychain_read_generic(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
}

fn persist_keychain_token(token: &str) -> Result<(), String> {
    keychain_write_generic(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, token)
}

fn clear_keychain_token() -> Result<(), String> {
    keychain_delete_generic(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
}

fn read_signing_settings_file() -> Option<Value> {
    let path = signing_settings_file_path()?;
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&raw).ok()
}

fn write_signing_settings_file(settings: &Value) -> Result<(), String> {
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
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms).map_err(|e| format!("chmod settings file: {e}"))?;
    }

    Ok(())
}

fn load_signing_settings() -> (Option<Value>, &'static str) {
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

fn persist_signing_settings(
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

fn user_presence_adapter() -> String {
    env::var("NEARXD_USER_PRESENCE_ADAPTER")
        .unwrap_or_else(|_| "auto".to_string())
        .trim()
        .to_ascii_lowercase()
}

#[cfg(target_os = "macos")]
fn run_swift_json(script: &str, args: &[&str]) -> Result<Value, String> {
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

#[cfg(target_os = "macos")]
fn keychain_write_generic_protected(
    service: &str,
    account: &str,
    value: &str,
    biometry_only: bool,
) -> Result<(), String> {
    let script = r#"
import Foundation
import Security

let service = CommandLine.arguments[1]
let account = CommandLine.arguments[2]
let value = CommandLine.arguments[3]
let biometryOnly = CommandLine.arguments.count > 4 ? (CommandLine.arguments[4] == "1") : true

guard let data = value.data(using: .utf8) else {
    fputs("failed to encode value", stderr)
    exit(2)
}

let deleteQuery: [String: Any] = [
    kSecClass as String: kSecClassGenericPassword,
    kSecAttrService as String: service,
    kSecAttrAccount as String: account
]
SecItemDelete(deleteQuery as CFDictionary)

// Try protected write (requires code-signed binary with entitlements)
let flags: SecAccessControlCreateFlags = biometryOnly ? .biometryCurrentSet : .userPresence
var acError: Unmanaged<CFError>?
if let access = SecAccessControlCreateWithFlags(nil, kSecAttrAccessibleWhenUnlockedThisDeviceOnly, flags, &acError) {
    let addQuery: [String: Any] = [
        kSecClass as String: kSecClassGenericPassword,
        kSecAttrService as String: service,
        kSecAttrAccount as String: account,
        kSecValueData as String: data,
        kSecAttrAccessControl as String: access
    ]
    let status = SecItemAdd(addQuery as CFDictionary, nil)
    if status == errSecSuccess {
        let out: [String: Any] = [
            "stored": true,
            "protection": biometryOnly ? "biometry_current_set" : "user_presence"
        ]
        let json = try JSONSerialization.data(withJSONObject: out, options: [])
        print(String(data: json, encoding: .utf8)!)
        exit(0)
    }
    // -34018 = errSecMissingEntitlement — binary not code-signed for protected keychain
    if status != -34018 {
        fputs("SecItemAdd failed with status \(status)", stderr)
        exit(3)
    }
    fputs("Protected SecItemAdd failed (-34018), falling back to unprotected keychain\n", stderr)
}

// Fallback: store without biometry protection (works for unsigned CLI binaries)
let fallbackQuery: [String: Any] = [
    kSecClass as String: kSecClassGenericPassword,
    kSecAttrService as String: service,
    kSecAttrAccount as String: account,
    kSecValueData as String: data,
    kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlocked
]
let fallbackStatus = SecItemAdd(fallbackQuery as CFDictionary, nil)
if fallbackStatus == errSecSuccess {
    let out: [String: Any] = [
        "stored": true,
        "protection": "when_unlocked",
        "fallback": true
    ]
    let json = try JSONSerialization.data(withJSONObject: out, options: [])
    print(String(data: json, encoding: .utf8)!)
    exit(0)
}

fputs("SecItemAdd fallback also failed with status \(fallbackStatus)", stderr)
exit(3)
"#;

    let _ = run_swift_json(
        script,
        &[
            service,
            account,
            value,
            if biometry_only { "1" } else { "0" },
        ],
    )?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn keychain_read_generic_with_prompt(
    service: &str,
    account: &str,
    reason: &str,
) -> Result<String, String> {
    let script = r#"
import Foundation
import Security
import LocalAuthentication

let service = CommandLine.arguments[1]
let account = CommandLine.arguments[2]
let reason = CommandLine.arguments.count > 3 ? CommandLine.arguments[3] : "Authenticate to continue"

let context = LAContext()
context.localizedReason = reason

let query: [String: Any] = [
    kSecClass as String: kSecClassGenericPassword,
    kSecAttrService as String: service,
    kSecAttrAccount as String: account,
    kSecReturnData as String: true,
    kSecMatchLimit as String: kSecMatchLimitOne,
    kSecUseAuthenticationContext as String: context
]

var item: CFTypeRef?
let status = SecItemCopyMatching(query as CFDictionary, &item)
if status == errSecSuccess, let data = item as? Data, let value = String(data: data, encoding: .utf8) {
    let out: [String: Any] = ["value": value]
    let json = try JSONSerialization.data(withJSONObject: out, options: [])
    print(String(data: json, encoding: .utf8)!)
    exit(0)
}

if status == errSecItemNotFound {
    fputs("keychain item not found", stderr)
    exit(2)
}

fputs("SecItemCopyMatching failed with status \(status)", stderr)
exit(3)
"#;

    let v = run_swift_json(script, &[service, account, reason])?;
    v.get("value")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "missing value field in keychain read response".to_string())
}

#[cfg(not(target_os = "macos"))]
fn keychain_write_generic_protected(
    _service: &str,
    _account: &str,
    _value: &str,
    _biometry_only: bool,
) -> Result<(), String> {
    Err("protected keychain writes are unavailable on this platform".to_string())
}

#[cfg(not(target_os = "macos"))]
fn keychain_read_generic_with_prompt(
    _service: &str,
    _account: &str,
    _reason: &str,
) -> Result<String, String> {
    Err("keychain prompt reads are unavailable on this platform".to_string())
}

#[cfg(target_os = "macos")]
fn swift_probe_user_presence(allow_fallback: bool) -> Result<Value, String> {
    let script = r#"
import Foundation
import LocalAuthentication

let allowFallback = CommandLine.arguments.count > 1 ? (CommandLine.arguments[1] == "1") : true
let context = LAContext()
var error: NSError?
var available = false
var modality = "none"

if #available(macOS 10.12.2, *) {
    if context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error) {
        available = true
        modality = "biometrics"
    }
}

if !available && allowFallback {
    if context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &error) {
        available = true
        modality = "device_owner_authentication"
    }
}

var out: [String: Any] = [
    "platform": "macos",
    "available": available,
    "modality": modality
]
if let e = error {
    out["detail"] = e.localizedDescription
}

let data = try JSONSerialization.data(withJSONObject: out, options: [])
print(String(data: data, encoding: .utf8)!)
"#;

    run_swift_json(script, &[if allow_fallback { "1" } else { "0" }])
}

#[cfg(target_os = "macos")]
fn swift_request_user_presence(reason: &str, allow_fallback: bool) -> Result<Value, String> {
    let script = r#"
import Foundation
import LocalAuthentication
import Dispatch

let reason = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "Approve action"
let allowFallback = CommandLine.arguments.count > 2 ? (CommandLine.arguments[2] == "1") : true

let context = LAContext()
var capabilityError: NSError?
var selectedPolicy: LAPolicy?
var modality = "none"

if #available(macOS 10.12.2, *) {
    if context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &capabilityError) {
        selectedPolicy = .deviceOwnerAuthenticationWithBiometrics
        modality = "biometrics"
    }
}

if selectedPolicy == nil && allowFallback {
    if context.canEvaluatePolicy(.deviceOwnerAuthentication, error: &capabilityError) {
        selectedPolicy = .deviceOwnerAuthentication
        modality = "device_owner_authentication"
    }
}

guard let policy = selectedPolicy else {
    let msg = capabilityError?.localizedDescription ?? "user presence unavailable"
    fputs(msg, stderr)
    exit(2)
}

let sem = DispatchSemaphore(value: 0)
var ok = false
var errText = ""

context.evaluatePolicy(policy, localizedReason: reason) { success, err in
    ok = success
    if let err = err {
        errText = err.localizedDescription
    }
    sem.signal()
}

if sem.wait(timeout: .now() + 60) == .timedOut {
    fputs("user presence timed out", stderr)
    exit(3)
}

if ok {
    let out: [String: Any] = [
        "verified": true,
        "platform": "macos",
        "modality": modality
    ]
    let data = try JSONSerialization.data(withJSONObject: out, options: [])
    print(String(data: data, encoding: .utf8)!)
    exit(0)
}

if errText.isEmpty {
    errText = "user presence rejected"
}
fputs(errText, stderr)
exit(3)
"#;

    run_swift_json(script, &[reason, if allow_fallback { "1" } else { "0" }])
}

fn probe_user_presence(allow_fallback: bool) -> Value {
    let adapter = user_presence_adapter();
    match adapter.as_str() {
        "mock" => json!({
            "adapter": "mock",
            "platform": std::env::consts::OS,
            "available": true,
            "modality": "mock",
        }),
        "none" => json!({
            "adapter": "none",
            "platform": std::env::consts::OS,
            "available": false,
            "modality": "none",
            "detail": "adapter disabled by NEARXD_USER_PRESENCE_ADAPTER=none",
        }),
        "swift" | "auto" => {
            #[cfg(target_os = "macos")]
            {
                match swift_probe_user_presence(allow_fallback) {
                    Ok(mut v) => {
                        if let Some(obj) = v.as_object_mut() {
                            obj.insert("adapter".to_string(), json!("swift"));
                        }
                        v
                    }
                    Err(e) => json!({
                        "adapter": "swift",
                        "platform": "macos",
                        "available": false,
                        "modality": "none",
                        "detail": e,
                    }),
                }
            }
            #[cfg(not(target_os = "macos"))]
            {
                json!({
                    "adapter": adapter,
                    "platform": std::env::consts::OS,
                    "available": false,
                    "modality": "none",
                    "detail": "swift biometric adapter is only available on macOS",
                })
            }
        }
        _ => json!({
            "adapter": adapter,
            "platform": std::env::consts::OS,
            "available": false,
            "modality": "none",
            "detail": "unknown NEARXD_USER_PRESENCE_ADAPTER value",
        }),
    }
}

fn request_user_presence(reason: &str, allow_fallback: bool) -> Result<Value, String> {
    let adapter = user_presence_adapter();
    match adapter.as_str() {
        "mock" => Ok(json!({
            "verified": true,
            "platform": std::env::consts::OS,
            "modality": "mock",
            "adapter": "mock",
        })),
        "none" => {
            Err("user presence adapter disabled (NEARXD_USER_PRESENCE_ADAPTER=none)".to_string())
        }
        "swift" | "auto" => {
            #[cfg(target_os = "macos")]
            {
                let mut v = swift_request_user_presence(reason, allow_fallback)?;
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("adapter".to_string(), json!("swift"));
                }
                Ok(v)
            }
            #[cfg(not(target_os = "macos"))]
            {
                Err("user presence is unavailable on this platform".to_string())
            }
        }
        _ => Err(format!(
            "unknown NEARXD_USER_PRESENCE_ADAPTER value: {adapter}"
        )),
    }
}

#[derive(Debug, Deserialize)]
struct NearCredentialFile {
    #[serde(default)]
    account_id: Option<String>,
    #[serde(default)]
    public_key: Option<String>,
    #[serde(default)]
    private_key: Option<String>,
    #[serde(default)]
    secret_key: Option<String>,
}

#[derive(Debug)]
struct ParsedNearCredential {
    account_id: String,
    public_key: String,
    private_key: String,
}

fn parse_near_credential(path: &Path) -> Result<ParsedNearCredential, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let parsed: NearCredentialFile =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;

    let fallback_account = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(str::to_string)
        .unwrap_or_default();

    let account_id = parsed
        .account_id
        .unwrap_or(fallback_account)
        .trim()
        .to_string();
    if account_id.is_empty() {
        return Err(format!(
            "missing account_id in credential file {}",
            path.display()
        ));
    }

    let public_key = parsed.public_key.unwrap_or_default().trim().to_string();
    if public_key.is_empty() {
        return Err(format!(
            "missing public_key in credential file {}",
            path.display()
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
            "missing private_key in credential file {}",
            path.display()
        ));
    }

    Ok(ParsedNearCredential {
        account_id,
        public_key,
        private_key,
    })
}

fn list_credential_summary(path: &Path) -> Result<(String, String), String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let parsed: NearCredentialFile =
        serde_json::from_str(&raw).map_err(|e| format!("parse {}: {e}", path.display()))?;
    let fallback = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    let account_id = parsed.account_id.unwrap_or(fallback).trim().to_string();
    let public_key = parsed.public_key.unwrap_or_default().trim().to_string();
    if account_id.is_empty() || public_key.is_empty() {
        return Err("missing account_id or public_key".to_string());
    }
    Ok((account_id, public_key))
}

fn collect_credential_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let entries = fs::read_dir(dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;

    for entry in entries {
        let entry = entry.map_err(|e| format!("read_dir entry error: {e}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        files.push(path);
    }

    files.sort();
    Ok(files)
}

fn store_near_credential_keychain(
    network: &str,
    cred: &ParsedNearCredential,
    overwrite: bool,
    protection: &str,
) -> Result<&'static str, String> {
    if !cfg!(target_os = "macos") {
        return Err("credential keychain storage is only supported on macOS".to_string());
    }

    let account = format!("{network}:{}", cred.account_id);
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

fn read_near_credential_keychain(
    network: &str,
    account_id: &str,
    reason: &str,
) -> Result<Value, String> {
    if !cfg!(target_os = "macos") {
        return Err("credential keychain reads are only supported on macOS".to_string());
    }

    let account = format!("{network}:{account_id}");
    let raw =
        keychain_read_generic_with_prompt(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &account, reason)?;

    let mut payload =
        serde_json::from_str::<Value>(&raw).map_err(|e| format!("decode stored payload: {e}"))?;
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("keychain_account".to_string(), json!(account));
    }
    Ok(payload)
}

fn resolve_fastnear_token(state: &BrokerState) -> (Option<String>, &'static str) {
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn random_urlsafe(len_bytes: usize) -> String {
    let mut bytes = vec![0u8; len_bytes];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn cleanup_expired_intents(state: &BrokerState) {
    let now = now_ms();
    if let Ok(mut intents) = state.sign_intents.lock() {
        intents.retain(|_, v| v.expires_at_ms > now);
    }
}

fn open_url(url: &str) -> Result<(), String> {
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

fn route_to_json(route: &nearx::router::Route) -> Value {
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
    }
}

fn handle_request(state: &Arc<BrokerState>, req: BrokerRequest) -> BrokerResponse {
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

        "list_near_credentials" => {
            let network = parse_string(&req.params, "network")
                .unwrap_or("mainnet")
                .trim()
                .to_ascii_lowercase();
            if network.is_empty() {
                return BrokerResponse::err(id, "ERR_PARAMS", "network cannot be empty");
            }

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

            let files = match collect_credential_files(&credentials_dir) {
                Ok(f) => f,
                Err(_) => {
                    return BrokerResponse::ok(
                        id,
                        json!({
                            "network": network,
                            "credentials_dir": dir_display,
                            "accounts": [],
                        }),
                    );
                }
            };

            let mut accounts = Vec::new();
            for file in &files {
                if let Ok((account_id, public_key)) = list_credential_summary(file) {
                    let keychain_account = format!("{network}:{account_id}");
                    let in_keychain =
                        keychain_has_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &keychain_account);
                    accounts.push(json!({
                        "account_id": account_id,
                        "public_key": public_key,
                        "in_keychain": in_keychain,
                    }));
                }
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

        "import_near_credentials" => {
            let network = parse_string(&req.params, "network")
                .unwrap_or("testnet")
                .trim()
                .to_ascii_lowercase();
            if network.is_empty() {
                return BrokerResponse::err(id, "ERR_PARAMS", "network cannot be empty");
            }

            let credentials_dir = if let Some(raw) = parse_string(&req.params, "credentials_dir") {
                expand_tilde_path(raw)
            } else {
                match near_credentials_dir(&network) {
                    Some(p) => p,
                    None => {
                        return BrokerResponse::err(
                            id,
                            "ERR_IO",
                            "unable to resolve default ~/.near-credentials path",
                        )
                    }
                }
            };

            if !credentials_dir.exists() {
                return BrokerResponse::err(
                    id,
                    "ERR_IO",
                    format!(
                        "credentials directory does not exist: {}",
                        credentials_dir.display()
                    ),
                );
            }

            let account_filter = parse_string(&req.params, "account_id").map(str::to_string);
            let keychain_protection = parse_string(&req.params, "keychain_credential_protection")
                .unwrap_or(DEFAULT_KEYCHAIN_CREDENTIAL_PROTECTION)
                .trim()
                .to_ascii_lowercase();
            if keychain_protection != "biometry_current_set"
                && keychain_protection != "user_presence"
            {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "keychain_credential_protection must be 'biometry_current_set' or 'user_presence'",
                );
            }

            let require_user_presence = parse_bool(
                &req.params,
                "require_user_presence",
                cfg!(target_os = "macos"),
            );
            let allow_fallback_default = keychain_protection != "biometry_current_set";
            let allow_fallback = parse_optional_bool(&req.params, "allow_fallback")
                .unwrap_or(allow_fallback_default);
            let reason = parse_string(&req.params, "reason")
                .unwrap_or("NEARx needs your approval to import NEAR credentials.");
            let persist_in_keychain = parse_bool(
                &req.params,
                "persist_in_keychain",
                cfg!(target_os = "macos"),
            );
            let overwrite = parse_bool(&req.params, "overwrite", false);
            let save_settings = parse_bool(&req.params, "save_settings", true);
            let max_accounts = parse_u64(&req.params, "max_accounts", 200).clamp(1, 1_000) as usize;

            if persist_in_keychain && !cfg!(target_os = "macos") {
                return BrokerResponse::err(
                    id,
                    "ERR_UNAVAILABLE",
                    "persist_in_keychain is only supported on macOS",
                );
            }

            let user_presence = if require_user_presence {
                match request_user_presence(reason, allow_fallback) {
                    Ok(v) => v,
                    Err(e) => return BrokerResponse::err(id, "ERR_AUTH", e),
                }
            } else {
                json!({
                    "verified": false,
                    "skipped": true,
                    "reason": "require_user_presence=false"
                })
            };

            let files = match collect_credential_files(&credentials_dir) {
                Ok(v) => v,
                Err(e) => return BrokerResponse::err(id, "ERR_IO", e),
            };

            let mut imported = Vec::<Value>::new();
            let mut skipped = Vec::<Value>::new();
            let mut failed = Vec::<Value>::new();

            for path in files {
                if imported.len() >= max_accounts {
                    skipped.push(json!({
                        "file": path.display().to_string(),
                        "reason": "max_accounts limit reached",
                    }));
                    continue;
                }

                let cred = match parse_near_credential(&path) {
                    Ok(v) => v,
                    Err(e) => {
                        failed.push(json!({
                            "file": path.display().to_string(),
                            "error": e,
                        }));
                        continue;
                    }
                };

                if let Some(filter) = account_filter.as_deref() {
                    if cred.account_id != filter {
                        continue;
                    }
                }

                let keychain_account = format!("{network}:{}", cred.account_id);
                let keychain_status = if persist_in_keychain {
                    match store_near_credential_keychain(
                        &network,
                        &cred,
                        overwrite,
                        &keychain_protection,
                    ) {
                        Ok(status) => status.to_string(),
                        Err(e) => {
                            failed.push(json!({
                                "file": path.display().to_string(),
                                "account_id": cred.account_id,
                                "error": e,
                            }));
                            continue;
                        }
                    }
                } else {
                    "not_requested".to_string()
                };

                imported.push(json!({
                    "account_id": cred.account_id,
                    "public_key": cred.public_key,
                    "file": path.display().to_string(),
                    "keychain_account": if persist_in_keychain { Some(keychain_account) } else { None::<String> },
                    "keychain_status": keychain_status,
                }));
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
                            "last_imported_at_ms": now_ms(),
                            "account_filter": account_filter,
                            "imported_accounts": imported
                                .iter()
                                .filter_map(|v| v.get("account_id").and_then(Value::as_str))
                                .collect::<Vec<_>>(),
                            "require_user_presence": require_user_presence,
                            "persist_in_keychain": persist_in_keychain,
                            "keychain_credential_protection": keychain_protection,
                        }),
                    );
                }

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

            BrokerResponse::ok(
                id,
                json!({
                    "network": network,
                    "credentials_dir": credentials_dir.display().to_string(),
                    "imported_count": imported.len(),
                    "imported": imported,
                    "skipped": skipped,
                    "failed": failed,
                    "user_presence": user_presence,
                    "settings_save": settings_save,
                    "keychain_credential_protection": if persist_in_keychain { Some(keychain_protection) } else { None::<String> },
                }),
            )
        }

        "get_near_credential" => {
            let network = parse_string(&req.params, "network")
                .unwrap_or("testnet")
                .trim()
                .to_ascii_lowercase();
            if network.is_empty() {
                return BrokerResponse::err(id, "ERR_PARAMS", "network cannot be empty");
            }

            let Some(account_id) = parse_string(&req.params, "account_id") else {
                return BrokerResponse::err(
                    id,
                    "ERR_PARAMS",
                    "missing required string param: account_id",
                );
            };
            let reason = parse_string(&req.params, "reason")
                .unwrap_or("NEARx needs your approval to access this credential.");

            match read_near_credential_keychain(&network, account_id, reason) {
                Ok(payload) => BrokerResponse::ok(
                    id,
                    json!({
                        "network": network,
                        "account_id": account_id,
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
                    return BrokerResponse::err(
                        id,
                        "ERR_PARAMS",
                        format!("invalid signer_id: {e}"),
                    )
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

            // Read credential from keychain (triggers Touch ID)
            let network = parse_string(&req.params, "network")
                .unwrap_or("mainnet")
                .to_ascii_lowercase();
            let reason = parse_string(&req.params, "reason")
                .unwrap_or("NEARx needs your approval to sign a transaction.");
            let credential =
                match read_near_credential_keychain(&network, signer_id_str, reason) {
                    Ok(v) => v,
                    Err(e) => {
                        return BrokerResponse::err(
                            id,
                            "ERR_AUTH",
                            format!("credential read failed: {e}"),
                        )
                    }
                };

            // Extract and parse private key
            let Some(private_key_str) = credential.get("private_key").and_then(Value::as_str)
            else {
                return BrokerResponse::err(
                    id,
                    "ERR_AUTH",
                    "credential missing 'private_key' field",
                );
            };
            let secret_key: SecretKey = match private_key_str.parse() {
                Ok(v) => v,
                Err(e) => {
                    return BrokerResponse::err(
                        id,
                        "ERR_AUTH",
                        format!("invalid private key format: {e}"),
                    )
                }
            };

            // Extract and parse public key
            let public_key: PublicKey = match credential
                .get("public_key")
                .and_then(Value::as_str)
                .map(str::parse)
            {
                Some(Ok(v)) => v,
                _ => {
                    // Derive from secret key if not stored
                    secret_key.public_key()
                }
            };

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
            let (tx_hash, _size) = tx.get_hash_and_size();
            let signature = secret_key.sign(tx_hash.as_ref());
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
                }),
            )
        }

        _ => BrokerResponse::err(id, "ERR_METHOD", format!("unknown method: {}", req.method)),
    }
}

trait CloneableStream: Read + Write + Send + 'static {
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

fn serve_connection<S>(stream: S, state: Arc<BrokerState>)
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
fn socket_path() -> PathBuf {
    if let Ok(path) = env::var("NEARXD_SOCKET_PATH") {
        let p = PathBuf::from(path.trim());
        if !p.as_os_str().is_empty() {
            return p;
        }
    }

    env::temp_dir().join("nearxd.sock")
}

#[cfg(unix)]
fn prepare_socket_path(path: &Path) -> Result<(), String> {
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
fn run_unix(state: Arc<BrokerState>) -> Result<(), String> {
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

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    #[cfg(unix)]
    {
        let state = Arc::new(BrokerState::default());
        if let Err(e) = run_unix(state) {
            eprintln!("nearxd failed: {e}");
            std::process::exit(1);
        }
    }

    #[cfg(not(unix))]
    {
        eprintln!("nearxd currently supports Unix-domain sockets only");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_user_presence_adapter<T>(adapter: &str, f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap();
        let old = std::env::var("NEARXD_USER_PRESENCE_ADAPTER").ok();
        std::env::set_var("NEARXD_USER_PRESENCE_ADAPTER", adapter);
        let out = f();
        if let Some(prev) = old {
            std::env::set_var("NEARXD_USER_PRESENCE_ADAPTER", prev);
        } else {
            std::env::remove_var("NEARXD_USER_PRESENCE_ADAPTER");
        }
        out
    }

    fn mktemp_dir(prefix: &str) -> PathBuf {
        let unique = format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        );
        let path = std::env::temp_dir().join(unique);
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn parse_deep_link_method_normalizes_legacy_scheme() {
        let state = Arc::new(BrokerState::default());
        let req = BrokerRequest {
            id: Some("x".to_string()),
            method: "parse_deep_link".to_string(),
            params: json!({"url": "near://block/178923456"}),
        };

        let resp = handle_request(&state, req);
        assert!(resp.ok);
        assert_eq!(
            resp.result
                .as_ref()
                .and_then(|v| v.get("canonical_url"))
                .and_then(|v| v.as_str()),
            Some("nearx://v1/block/178923456")
        );
    }

    #[test]
    fn unknown_method_returns_error() {
        let state = Arc::new(BrokerState::default());
        let req = BrokerRequest {
            id: Some("x".to_string()),
            method: "not_a_method".to_string(),
            params: json!({}),
        };

        let resp = handle_request(&state, req);
        assert!(!resp.ok);
        assert_eq!(resp.error.as_ref().map(|e| e.code), Some("ERR_METHOD"));
    }

    #[test]
    fn sign_intent_happy_path_create_approve_consume() {
        let state = Arc::new(BrokerState::default());

        let create = handle_request(
            &state,
            BrokerRequest {
                id: Some("1".to_string()),
                method: "create_sign_intent".to_string(),
                params: json!({
                    "account_id": "alice.near",
                    "payload": { "kind": "tx", "nonce": 42 },
                    "origin": "e2e"
                }),
            },
        );
        assert!(create.ok);

        let intent_id = create
            .result
            .as_ref()
            .and_then(|v| v.get("intent_id"))
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        let challenge = create
            .result
            .as_ref()
            .and_then(|v| v.get("challenge"))
            .and_then(Value::as_str)
            .unwrap()
            .to_string();

        let approve = handle_request(
            &state,
            BrokerRequest {
                id: Some("2".to_string()),
                method: "approve_sign_intent".to_string(),
                params: json!({
                    "intent_id": intent_id.clone(),
                    "challenge": challenge.clone()
                }),
            },
        );
        assert!(approve.ok);

        let consume = handle_request(
            &state,
            BrokerRequest {
                id: Some("3".to_string()),
                method: "consume_sign_intent".to_string(),
                params: json!({
                    "intent_id": intent_id,
                    "challenge": challenge
                }),
            },
        );
        assert!(consume.ok);
        assert_eq!(
            consume
                .result
                .as_ref()
                .and_then(|v| v.get("account_id"))
                .and_then(Value::as_str),
            Some("alice.near")
        );
    }

    #[test]
    fn sign_intent_rejects_wrong_challenge() {
        let state = Arc::new(BrokerState::default());

        let create = handle_request(
            &state,
            BrokerRequest {
                id: Some("1".to_string()),
                method: "create_sign_intent".to_string(),
                params: json!({
                    "account_id": "alice.near",
                    "payload": { "kind": "tx" }
                }),
            },
        );
        assert!(create.ok);

        let intent_id = create
            .result
            .as_ref()
            .and_then(|v| v.get("intent_id"))
            .and_then(Value::as_str)
            .unwrap()
            .to_string();

        let approve = handle_request(
            &state,
            BrokerRequest {
                id: Some("2".to_string()),
                method: "approve_sign_intent".to_string(),
                params: json!({
                    "intent_id": intent_id,
                    "challenge": "wrong"
                }),
            },
        );
        assert!(!approve.ok);
        assert_eq!(approve.error.as_ref().map(|e| e.code), Some("ERR_AUTH"));
    }

    #[test]
    fn sign_intent_user_presence_mock_adapter() {
        with_user_presence_adapter("mock", || {
            let state = Arc::new(BrokerState::default());

            let create = handle_request(
                &state,
                BrokerRequest {
                    id: Some("1".to_string()),
                    method: "create_sign_intent".to_string(),
                    params: json!({
                        "account_id": "alice.near",
                        "payload": { "kind": "tx" },
                        "require_user_presence": true
                    }),
                },
            );
            assert!(create.ok);

            let intent_id = create
                .result
                .as_ref()
                .and_then(|v| v.get("intent_id"))
                .and_then(Value::as_str)
                .unwrap()
                .to_string();
            let challenge = create
                .result
                .as_ref()
                .and_then(|v| v.get("challenge"))
                .and_then(Value::as_str)
                .unwrap()
                .to_string();

            let approve = handle_request(
                &state,
                BrokerRequest {
                    id: Some("2".to_string()),
                    method: "approve_sign_intent".to_string(),
                    params: json!({
                        "intent_id": intent_id,
                        "challenge": challenge
                    }),
                },
            );
            assert!(approve.ok);
            assert_eq!(
                approve
                    .result
                    .as_ref()
                    .and_then(|v| v.get("user_presence_verified"))
                    .and_then(Value::as_bool),
                Some(true)
            );
        });
    }

    #[test]
    fn import_near_credentials_from_custom_dir_without_keychain() {
        let tmp = mktemp_dir("nearxd-import-test");
        let creds = tmp.join("alice.testnet.json");
        fs::write(
            &creds,
            r#"{
                "account_id":"alice.testnet",
                "public_key":"ed25519:ABC123",
                "private_key":"ed25519:SECRET"
            }"#,
        )
        .unwrap();

        let state = Arc::new(BrokerState::default());
        let resp = handle_request(
            &state,
            BrokerRequest {
                id: Some("x".to_string()),
                method: "import_near_credentials".to_string(),
                params: json!({
                    "network": "testnet",
                    "credentials_dir": tmp.display().to_string(),
                    "require_user_presence": false,
                    "persist_in_keychain": false,
                    "save_settings": false,
                }),
            },
        );

        assert!(resp.ok);
        assert_eq!(
            resp.result
                .as_ref()
                .and_then(|v| v.get("imported_count"))
                .and_then(Value::as_u64),
            Some(1)
        );

        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn import_near_credentials_rejects_invalid_protection_mode() {
        let tmp = mktemp_dir("nearxd-import-invalid-protection");
        let state = Arc::new(BrokerState::default());
        let resp = handle_request(
            &state,
            BrokerRequest {
                id: Some("x".to_string()),
                method: "import_near_credentials".to_string(),
                params: json!({
                    "network": "testnet",
                    "credentials_dir": tmp.display().to_string(),
                    "keychain_credential_protection": "invalid_mode"
                }),
            },
        );

        assert!(!resp.ok);
        assert_eq!(resp.error.as_ref().map(|e| e.code), Some("ERR_PARAMS"));
        let _ = fs::remove_dir_all(tmp);
    }

    #[test]
    fn get_near_credential_requires_account_id() {
        let state = Arc::new(BrokerState::default());
        let resp = handle_request(
            &state,
            BrokerRequest {
                id: Some("x".to_string()),
                method: "get_near_credential".to_string(),
                params: json!({
                    "network": "testnet"
                }),
            },
        );
        assert!(!resp.ok);
        assert_eq!(resp.error.as_ref().map(|e| e.code), Some("ERR_PARAMS"));
    }
}
