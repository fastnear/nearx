#![cfg(test)]

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use near_crypto::SecretKey;
use near_primitives::action::Action;
use near_primitives::transaction::{SignedTransaction, Transaction};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::broker::{handle_request, BrokerRequest, BrokerState};
use crate::credentials::{
    nearxd_credential_account_key, nearxd_credential_account_legacy,
    KEYCHAIN_NEAR_CREDENTIAL_SERVICE, SOURCE_HARDWARE_WALLET, SOURCE_LEGACY_FILE,
    SOURCE_NEARXD_KEYCHAIN, SOURCE_NEAR_CLI_SECURE,
};
use crate::hardware_wallet::{
    mock_ledger_get_public_key, DEFAULT_LEDGER_DERIVATION_PATH, HARDWARE_WALLET_TYPE_LEDGER,
};
use crate::keychain::{
    keychain_delete_generic, keychain_has_generic, keychain_write_generic,
};
use crate::settings::{
    staking_watchlist_for_network, write_signing_settings_file, HARDWARE_WALLET_INDEX_KEY,
    HARDWARE_WALLET_INDEX_VERSION, SIGNING_KEY_INDEX_KEY, SIGNING_KEY_INDEX_VERSION,
    SIGNING_KEY_PRUNE_AFTER_MS, SIGNING_KEY_STALE_AFTER_MS, STAKING_WATCHLIST_KEY,
};
use crate::signing::{build_signing_keys, discover_signing_accounts};
use crate::util::now_ms;

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

fn with_env_var<T>(key: &str, value: Option<&str>, f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap();
    let old = std::env::var(key).ok();
    match value {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    }
    let out = f();
    if let Some(prev) = old {
        std::env::set_var(key, prev);
    } else {
        std::env::remove_var(key);
    }
    out
}

fn with_env_vars<T>(vars: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
    let _guard = ENV_LOCK.lock().unwrap();
    let mut old = Vec::with_capacity(vars.len());
    for (key, value) in vars {
        old.push(((*key).to_string(), std::env::var(key).ok()));
        match value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
    let out = f();
    for (key, prev) in old.into_iter().rev() {
        if let Some(v) = prev {
            std::env::set_var(key, v);
        } else {
            std::env::remove_var(key);
        }
    }
    out
}

fn implicit_account_id_from_public_key(public_key: &near_crypto::PublicKey) -> String {
    public_key
        .key_data()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn is_skippable_keychain_error(err: &str) -> bool {
    let lower = err.to_ascii_lowercase();
    lower.contains("authorization was canceled")
        || lower.contains("user interaction is not allowed")
        || lower.contains("interaction not allowed")
        || lower.contains("keychain backend unavailable")
}

fn maybe_skip_keychain_test(test_name: &str, err: &str) -> bool {
    if is_skippable_keychain_error(err) {
        eprintln!("[nearxd:test-skip] {test_name}: {err}");
        true
    } else {
        false
    }
}

#[cfg(target_os = "macos")]
fn keychain_tests_enabled() -> bool {
    let Ok(raw) = std::env::var("NEARXD_RUN_KEYCHAIN_TESTS") else {
        return false;
    };
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes"
    )
}

#[cfg(target_os = "macos")]
fn maybe_skip_keychain_integration_test(test_name: &str) -> bool {
    if keychain_tests_enabled() {
        return false;
    }
    eprintln!(
        "[nearxd:test-skip] {test_name}: set NEARXD_RUN_KEYCHAIN_TESTS=1 to enable macOS keychain integration tests"
    );
    true
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

fn unique_test_account_id() -> String {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("nearx-{}-{}.testnet", std::process::id(), suffix)
}

fn mock_view_access_key_list_response(public_key: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": "nearxd-test",
        "result": {
            "keys": [
                {
                    "public_key": public_key,
                    "access_key": {
                        "nonce": 1,
                        "permission": "FullAccess"
                    }
                }
            ]
        }
    })
}

fn spawn_mock_rpc_server(response: Value) -> (String, std::thread::JoinHandle<()>) {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock rpc listener");
    let addr = listener.local_addr().expect("mock rpc local addr");
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut request_buf = [0u8; 16 * 1024];
            let _ = stream.read(&mut request_buf);

            let body = serde_json::to_string(&response).expect("encode mock rpc response");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    (format!("http://{}", addr), handle)
}

#[cfg(target_os = "macos")]
fn near_cli_secure_service(network: &str, account_id: &str) -> String {
    format!("near-{network}-{account_id}")
}

#[cfg(target_os = "macos")]
fn near_cli_secure_account(account_id: &str, public_key: &str) -> String {
    format!("{account_id}:{public_key}")
}

#[cfg(target_os = "macos")]
fn write_near_cli_secure_test_credential_with_key_type(
    network: &str,
    account_id: &str,
    key_type: near_crypto::KeyType,
    seed: &str,
) -> Result<(String, String, String, String), String> {
    let secret = SecretKey::from_seed(key_type, seed);
    let public_key = secret.public_key().to_string();
    let private_key = secret.to_string();
    let service = near_cli_secure_service(network, account_id);
    let account = near_cli_secure_account(account_id, &public_key);
    let payload = json!({
        "account_id": account_id,
        "public_key": public_key,
        "private_key": private_key,
    });
    let encoded = serde_json::to_string(&payload).expect("encode near-cli credential");
    keychain_write_generic(&service, &account, &encoded)
        .map_err(|e| format!("write near-cli credential failed: {e}"))?;
    Ok((service, account, public_key, private_key))
}

#[cfg(target_os = "macos")]
fn write_near_cli_secure_test_credential(
    network: &str,
    account_id: &str,
    seed: &str,
) -> Result<(String, String, String, String), String> {
    write_near_cli_secure_test_credential_with_key_type(
        network,
        account_id,
        near_crypto::KeyType::ED25519,
        seed,
    )
}

#[cfg(target_os = "macos")]
fn write_legacy_nearxd_test_credential(
    network: &str,
    account_id: &str,
    seed: &str,
) -> Result<(String, String, String, String), String> {
    let secret = SecretKey::from_seed(near_crypto::KeyType::ED25519, seed);
    let public_key = secret.public_key().to_string();
    let private_key = secret.to_string();
    let legacy_account = nearxd_credential_account_legacy(network, account_id);
    let payload = json!({
        "network": network,
        "account_id": account_id,
        "public_key": public_key,
        "private_key": private_key,
        "imported_at_ms": now_ms(),
    });
    let encoded = serde_json::to_string(&payload).expect("encode legacy nearxd credential");
    keychain_write_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &legacy_account, &encoded)
        .map_err(|e| format!("write legacy nearxd credential failed: {e}"))?;
    Ok((legacy_account, public_key, private_key, payload.to_string()))
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

#[test]
fn get_near_credential_near_cli_secure_requires_public_key() {
    let state = Arc::new(BrokerState::default());
    let resp = handle_request(
        &state,
        BrokerRequest {
            id: Some("x".to_string()),
            method: "get_near_credential".to_string(),
            params: json!({
                "network": "testnet",
                "account_id": "alice.testnet",
                "credential_source": "near_cli_secure"
            }),
        },
    );
    assert!(!resp.ok);
    assert_eq!(resp.error.as_ref().map(|e| e.code), Some("ERR_PARAMS"));
}

#[test]
fn get_near_credential_reads_legacy_file_directly() {
    let home = mktemp_dir("nearxd-legacy-file-read");
    let home_str = home.display().to_string();
    let credentials_home = home.join(".near-credentials");
    let network_dir = credentials_home.join("testnet");
    fs::create_dir_all(&network_dir).expect("create credentials dir");

    let account_id = unique_test_account_id();
    let secret = SecretKey::from_seed(near_crypto::KeyType::ED25519, "nearx-legacy-read-seed");
    let public_key = secret.public_key().to_string();
    fs::write(
        network_dir.join(format!("{account_id}.json")),
        serde_json::to_string_pretty(&json!({
            "account_id": account_id,
            "public_key": public_key,
            "private_key": secret.to_string(),
        }))
        .expect("encode legacy credential"),
    )
    .expect("write legacy credential");

    with_env_var("HOME", Some(home_str.as_str()), || {
        let state = Arc::new(BrokerState::default());
        let resp = handle_request(
            &state,
            BrokerRequest {
                id: Some("legacy-read".to_string()),
                method: "get_near_credential".to_string(),
                params: json!({
                    "network": "testnet",
                    "account_id": account_id,
                    "public_key": public_key,
                    "credential_source": SOURCE_LEGACY_FILE,
                }),
            },
        );
        assert!(resp.ok);
        assert_eq!(
            resp.result
                .as_ref()
                .and_then(|v| v.get("credential_source"))
                .and_then(Value::as_str),
            Some(SOURCE_LEGACY_FILE)
        );
        assert_eq!(
            resp.result
                .as_ref()
                .and_then(|v| v.get("credential"))
                .and_then(|v| v.get("public_key"))
                .and_then(Value::as_str),
            Some(public_key.as_str())
        );
    });

    let _ = fs::remove_dir_all(home);
}

#[test]
fn sign_transaction_falls_back_to_legacy_file_when_it_is_the_only_source() {
    let home = mktemp_dir("nearxd-legacy-sign");
    let home_str = home.display().to_string();
    let credentials_home = home.join(".near-credentials");
    let network_dir = credentials_home.join("testnet");
    fs::create_dir_all(&network_dir).expect("create credentials dir");

    let account_id = unique_test_account_id();
    let secret = SecretKey::from_seed(near_crypto::KeyType::ED25519, "nearx-legacy-sign-seed");
    let public_key = secret.public_key().to_string();
    fs::write(
        network_dir.join(format!("{account_id}.json")),
        serde_json::to_string_pretty(&json!({
            "account_id": account_id,
            "public_key": public_key,
            "private_key": secret.to_string(),
        }))
        .expect("encode legacy credential"),
    )
    .expect("write legacy credential");

    with_env_vars(
        &[
            ("HOME", Some(home_str.as_str())),
            ("NEAR_NODE_URL", Some("http://127.0.0.1:1")),
        ],
        || {
            let state = Arc::new(BrokerState::default());
            let resp = handle_request(
                &state,
                BrokerRequest {
                    id: Some("legacy-sign".to_string()),
                    method: "sign_transaction".to_string(),
                    params: json!({
                        "signer_id": account_id,
                        "signer_public_key": public_key,
                        "receiver_id": "receiver.testnet",
                        "nonce": 1,
                        "block_hash": "11111111111111111111111111111111",
                        "actions": [{ "type": "Transfer", "deposit": "1" }],
                        "network": "testnet",
                        "reason": "test legacy sign fallback",
                    }),
                },
            );
            assert!(resp.ok);
            assert_eq!(
                resp.result
                    .as_ref()
                    .and_then(|v| v.get("credential_source"))
                    .and_then(Value::as_str),
                Some(SOURCE_LEGACY_FILE)
            );
            assert_eq!(
                resp.result
                    .as_ref()
                    .and_then(|v| v.get("public_key"))
                    .and_then(Value::as_str),
                Some(public_key.as_str())
            );
        },
    );

    let _ = fs::remove_dir_all(home);
}

#[cfg(target_os = "macos")]
#[test]
fn get_near_credential_reads_near_cli_secure_directly() {
    if maybe_skip_keychain_integration_test("get_near_credential_reads_near_cli_secure_directly") {
        return;
    }
    let account_id = unique_test_account_id();
    let (service, keychain_account, public_key, private_key) =
        match write_near_cli_secure_test_credential(
            "testnet",
            &account_id,
            "nearx-direct-read-seed",
        ) {
            Ok(v) => v,
            Err(err) => {
                if maybe_skip_keychain_test(
                    "get_near_credential_reads_near_cli_secure_directly",
                    &err,
                ) {
                    return;
                }
                panic!("{err}");
            }
        };

    let state = Arc::new(BrokerState::default());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let resp = handle_request(
            &state,
            BrokerRequest {
                id: Some("x".to_string()),
                method: "get_near_credential".to_string(),
                params: json!({
                    "network": "testnet",
                    "account_id": account_id,
                    "public_key": public_key,
                    "credential_source": "near_cli_secure",
                }),
            },
        );
        assert!(resp.ok);
        assert_eq!(
            resp.result
                .as_ref()
                .and_then(|v| v.get("credential_source"))
                .and_then(Value::as_str),
            Some("near_cli_secure")
        );
        assert_eq!(
            resp.result
                .as_ref()
                .and_then(|v| v.get("credential"))
                .and_then(|v| v.get("public_key"))
                .and_then(Value::as_str),
            Some(public_key.as_str())
        );
        assert_eq!(
            resp.result
                .as_ref()
                .and_then(|v| v.get("credential"))
                .and_then(|v| v.get("private_key"))
                .and_then(Value::as_str),
            Some(private_key.as_str())
        );
    }));

    let _ = keychain_delete_generic(&service, &keychain_account);
    if let Err(err) = result {
        std::panic::resume_unwind(err);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn sign_transaction_uses_explicit_signer_public_key_from_near_cli_secure() {
    if maybe_skip_keychain_integration_test(
        "sign_transaction_uses_explicit_signer_public_key_from_near_cli_secure",
    ) {
        return;
    }
    let account_id = unique_test_account_id();
    let (service1, keychain_account1, public_key1, _) = match write_near_cli_secure_test_credential(
        "testnet",
        &account_id,
        "nearx-explicit-key-seed-1",
    ) {
        Ok(v) => v,
        Err(err) => {
            if maybe_skip_keychain_test(
                "sign_transaction_uses_explicit_signer_public_key_from_near_cli_secure",
                &err,
            ) {
                return;
            }
            panic!("{err}");
        }
    };
    let (service2, keychain_account2, public_key2, _) = match write_near_cli_secure_test_credential(
        "testnet",
        &account_id,
        "nearx-explicit-key-seed-2",
    ) {
        Ok(v) => v,
        Err(err) => {
            let _ = keychain_delete_generic(&service1, &keychain_account1);
            if maybe_skip_keychain_test(
                "sign_transaction_uses_explicit_signer_public_key_from_near_cli_secure",
                &err,
            ) {
                return;
            }
            panic!("{err}");
        }
    };

    let state = Arc::new(BrokerState::default());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for pk in [&public_key1, &public_key2] {
            let resp = handle_request(
                &state,
                BrokerRequest {
                    id: Some("x".to_string()),
                    method: "sign_transaction".to_string(),
                    params: json!({
                        "signer_id": account_id,
                        "signer_public_key": pk,
                        "credential_source": "near_cli_secure",
                        "receiver_id": "receiver.testnet",
                        "nonce": 1,
                        "block_hash": "11111111111111111111111111111111",
                        "actions": [{ "type": "Transfer", "deposit": "1" }],
                        "network": "testnet",
                        "reason": "test explicit signer_public_key",
                    }),
                },
            );

            assert!(resp.ok);
            assert_eq!(
                resp.result
                    .as_ref()
                    .and_then(|v| v.get("credential_source"))
                    .and_then(Value::as_str),
                Some("near_cli_secure")
            );
            assert_eq!(
                resp.result
                    .as_ref()
                    .and_then(|v| v.get("public_key"))
                    .and_then(Value::as_str),
                Some(pk.as_str())
            );
        }
    }));

    let _ = keychain_delete_generic(&service1, &keychain_account1);
    let _ = keychain_delete_generic(&service2, &keychain_account2);
    if let Err(err) = result {
        std::panic::resume_unwind(err);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn get_near_credential_rejects_legacy_fallback_public_key_mismatch() {
    if maybe_skip_keychain_integration_test(
        "get_near_credential_rejects_legacy_fallback_public_key_mismatch",
    ) {
        return;
    }
    use near_crypto::KeyType;
    let account_id = unique_test_account_id();
    let legacy_account = nearxd_credential_account_legacy("testnet", &account_id);
    let secret_1 = SecretKey::from_seed(KeyType::ED25519, "nearx-legacy-match-1");
    let secret_2 = SecretKey::from_seed(KeyType::ED25519, "nearx-legacy-match-2");
    let payload = json!({
        "network": "testnet",
        "account_id": account_id,
        "public_key": secret_1.public_key().to_string(),
        "private_key": secret_1.to_string(),
    });
    let encoded = serde_json::to_string(&payload).expect("encode legacy payload");
    if let Err(err) =
        keychain_write_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &legacy_account, &encoded)
    {
        if maybe_skip_keychain_test(
            "get_near_credential_rejects_legacy_fallback_public_key_mismatch",
            &err,
        ) {
            return;
        }
        panic!("{err}");
    }

    let state = Arc::new(BrokerState::default());
    let expected_public_key = secret_2.public_key().to_string();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let resp = handle_request(
            &state,
            BrokerRequest {
                id: Some("x".to_string()),
                method: "get_near_credential".to_string(),
                params: json!({
                    "network": "testnet",
                    "account_id": account_id,
                    "public_key": expected_public_key,
                }),
            },
        );
        assert!(!resp.ok);
        assert_eq!(resp.error.as_ref().map(|e| e.code), Some("ERR_AUTH"));
        let msg = resp
            .error
            .as_ref()
            .map(|e| e.message.clone())
            .unwrap_or_default();
        assert!(msg.contains("does not match requested public_key"));
    }));

    let _ = keychain_delete_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &legacy_account);
    if let Err(err) = result {
        std::panic::resume_unwind(err);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn list_near_signing_keys_reports_legacy_nearxd_keychain_source_and_backfills_scoped_copy() {
    if maybe_skip_keychain_integration_test(
        "list_near_signing_keys_reports_legacy_nearxd_keychain_source_and_backfills_scoped_copy",
    ) {
        return;
    }
    let home = mktemp_dir("nearxd-legacy-keychain-list");
    let credentials_home = home.join(".near-credentials");
    let network_dir = credentials_home.join("testnet");
    fs::create_dir_all(&network_dir).expect("create credentials dir");

    let account_id = unique_test_account_id();
    let (legacy_account, public_key, private_key, _) = match write_legacy_nearxd_test_credential(
        "testnet",
        &account_id,
        "nearx-legacy-keychain-list-seed",
    ) {
        Ok(v) => v,
        Err(err) => {
            let _ = fs::remove_dir_all(&home);
            if maybe_skip_keychain_test(
                "list_near_signing_keys_reports_legacy_nearxd_keychain_source_and_backfills_scoped_copy",
                &err,
            ) {
                return;
            }
            panic!("{err}");
        }
    };
    fs::write(
        network_dir.join(format!("{account_id}.json")),
        serde_json::to_string_pretty(&json!({
            "account_id": account_id,
            "public_key": public_key,
            "private_key": private_key,
        }))
        .expect("encode legacy credential file"),
    )
    .expect("write legacy credential file");

    let (rpc_url, rpc_handle) =
        spawn_mock_rpc_server(mock_view_access_key_list_response(&public_key));
    let scoped_account = nearxd_credential_account_key("testnet", &account_id, &public_key);
    let state = Arc::new(BrokerState::default());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_env_var("NEAR_NODE_URL", Some(rpc_url.as_str()), || {
            let resp = handle_request(
                &state,
                BrokerRequest {
                    id: Some("legacy-list".to_string()),
                    method: "list_near_signing_keys".to_string(),
                    params: json!({
                        "network": "testnet",
                        "credentials_home_dir": credentials_home.display().to_string(),
                        "account_id": account_id,
                    }),
                },
            );
            assert!(resp.ok);
            let row = resp
                .result
                .as_ref()
                .and_then(|v| v.get("keys"))
                .and_then(Value::as_array)
                .and_then(|keys| {
                    keys.iter().find(|entry| {
                        entry
                            .get("public_key")
                            .and_then(Value::as_str)
                            .map(|value| value == public_key)
                            .unwrap_or(false)
                    })
                })
                .expect("expected signing key row");
            let available_sources = row
                .get("available_sources")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            assert_eq!(
                available_sources,
                vec![json!(SOURCE_NEARXD_KEYCHAIN), json!(SOURCE_LEGACY_FILE)]
            );
            assert_eq!(
                row.get("preferred_source").and_then(Value::as_str),
                Some(SOURCE_NEARXD_KEYCHAIN)
            );
            assert_eq!(
                row.get("in_nearxd_keychain").and_then(Value::as_bool),
                Some(true)
            );
            assert_eq!(
                row.get("nearxd_keychain_protection").and_then(Value::as_str),
                Some("unknown")
            );
            assert!(keychain_has_generic(
                KEYCHAIN_NEAR_CREDENTIAL_SERVICE,
                &scoped_account
            ));
        });
    }));

    let _ = keychain_delete_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &legacy_account);
    let _ = keychain_delete_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &scoped_account);
    let _ = fs::remove_dir_all(&home);
    let _ = rpc_handle.join();
    if let Err(err) = result {
        std::panic::resume_unwind(err);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn sign_transaction_falls_back_from_unprotected_keychain_for_explicit_keychain_source() {
    if maybe_skip_keychain_integration_test(
        "sign_transaction_falls_back_from_unprotected_keychain_for_explicit_keychain_source",
    ) {
        return;
    }

    let account_id = unique_test_account_id();
    let (legacy_account, public_key, _private_key, _) = match write_legacy_nearxd_test_credential(
        "testnet",
        &account_id,
        "nearx-keychain-fallback-sign-seed",
    ) {
        Ok(v) => v,
        Err(err) => {
            if maybe_skip_keychain_test(
                "sign_transaction_falls_back_from_unprotected_keychain_for_explicit_keychain_source",
                &err,
            ) {
                return;
            }
            panic!("{err}");
        }
    };

    let (rpc_url, rpc_handle) =
        spawn_mock_rpc_server(mock_view_access_key_list_response(&public_key));
    let scoped_account = nearxd_credential_account_key("testnet", &account_id, &public_key);
    let state = Arc::new(BrokerState::default());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_env_var("NEAR_NODE_URL", Some(rpc_url.as_str()), || {
            let resp = handle_request(
                &state,
                BrokerRequest {
                    id: Some("x".to_string()),
                    method: "sign_transaction".to_string(),
                    params: json!({
                        "network": "testnet",
                        "signer_id": account_id,
                        "signer_public_key": public_key,
                        "credential_source": SOURCE_NEARXD_KEYCHAIN,
                        "receiver_id": "receiver.testnet",
                        "nonce": 2u64,
                        "block_hash": "11111111111111111111111111111111",
                        "actions": [
                            {
                                "type": "Transfer",
                                "deposit": "1",
                            }
                        ],
                    }),
                },
            );
            // With no fallback sources available (legacy_file/near_cli_secure),
            // the broker returns ERR_AUTH when the keychain copy lacks biometric protection.
            assert!(!resp.ok);
            assert_eq!(
                resp.error.as_ref().map(|e| e.code),
                Some("ERR_AUTH")
            );
        });
    }));

    let _ = keychain_delete_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &legacy_account);
    let _ = keychain_delete_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &scoped_account);
    let _ = rpc_handle.join();
    if let Err(err) = result {
        std::panic::resume_unwind(err);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn list_near_signing_accounts_includes_legacy_nearxd_keychain_only_account() {
    if maybe_skip_keychain_integration_test(
        "list_near_signing_accounts_includes_legacy_nearxd_keychain_only_account",
    ) {
        return;
    }
    let home = mktemp_dir("nearxd-legacy-keychain-account-discovery");
    let credentials_home = home.join(".near-credentials");
    fs::create_dir_all(&credentials_home).expect("create credentials home");

    let account_id = unique_test_account_id();
    let (legacy_account, public_key, _, _) = match write_legacy_nearxd_test_credential(
        "testnet",
        &account_id,
        "nearx-legacy-keychain-account-discovery-seed",
    ) {
        Ok(v) => v,
        Err(err) => {
            let _ = fs::remove_dir_all(&home);
            if maybe_skip_keychain_test(
                "list_near_signing_accounts_includes_legacy_nearxd_keychain_only_account",
                &err,
            ) {
                return;
            }
            panic!("{err}");
        }
    };

    let discovered = discover_signing_accounts("testnet", &credentials_home, None);
    let sources = discovered
        .get(&account_id)
        .cloned()
        .expect("expected discovered account");
    assert!(sources.contains(SOURCE_NEARXD_KEYCHAIN));

    let state = Arc::new(BrokerState::default());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let resp = handle_request(
            &state,
            BrokerRequest {
                id: Some("legacy-account-list".to_string()),
                method: "list_near_signing_accounts".to_string(),
                params: json!({
                    "network": "testnet",
                    "credentials_home_dir": credentials_home.display().to_string(),
                }),
            },
        );
        assert!(resp.ok);
        let row = resp
            .result
            .as_ref()
            .and_then(|v| v.get("accounts"))
            .and_then(Value::as_array)
            .and_then(|accounts| {
                accounts.iter().find(|entry| {
                    entry
                        .get("account_id")
                        .and_then(Value::as_str)
                        .map(|value| value == account_id)
                        .unwrap_or(false)
                })
            })
            .expect("expected signing account row");
        let source_hints = row
            .get("source_hints")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        assert!(source_hints
            .iter()
            .any(|value| value.as_str() == Some(SOURCE_NEARXD_KEYCHAIN)));
        assert_eq!(row.get("has_keys").and_then(Value::as_bool), Some(true));
    }));

    let _ = keychain_delete_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &legacy_account);
    let _ = keychain_delete_generic(
        KEYCHAIN_NEAR_CREDENTIAL_SERVICE,
        &nearxd_credential_account_key("testnet", &account_id, &public_key),
    );
    let _ = fs::remove_dir_all(&home);
    if let Err(err) = result {
        std::panic::resume_unwind(err);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn build_signing_keys_does_not_attach_mismatched_legacy_nearxd_keychain_source() {
    if maybe_skip_keychain_integration_test(
        "build_signing_keys_does_not_attach_mismatched_legacy_nearxd_keychain_source",
    ) {
        return;
    }
    let home = mktemp_dir("nearxd-legacy-keychain-mismatch");
    let credentials_home = home.join(".near-credentials");
    fs::create_dir_all(&credentials_home).expect("create credentials home");

    let account_id = unique_test_account_id();
    let (legacy_account, mismatched_public_key, _, _) = match write_legacy_nearxd_test_credential(
        "testnet",
        &account_id,
        "nearx-legacy-keychain-mismatch-seed-a",
    ) {
        Ok(v) => v,
        Err(err) => {
            let _ = fs::remove_dir_all(&home);
            if maybe_skip_keychain_test(
                "build_signing_keys_does_not_attach_mismatched_legacy_nearxd_keychain_source",
                &err,
            ) {
                return;
            }
            panic!("{err}");
        }
    };
    let other_public_key = SecretKey::from_seed(
        near_crypto::KeyType::ED25519,
        "nearx-legacy-keychain-mismatch-seed-b",
    )
    .public_key()
    .to_string();

    let settings = json!({
        SIGNING_KEY_INDEX_KEY: {
            "version": SIGNING_KEY_INDEX_VERSION,
            "records": [{
                "network": "testnet",
                "account_id": account_id,
                "public_key": other_public_key,
                "available_sources": [SOURCE_NEARXD_KEYCHAIN],
                "in_nearxd_keychain": true,
                "last_seen_at_ms": now_ms(),
            }],
        },
    });

    let keys = with_env_var("NEAR_NODE_URL", Some("http://127.0.0.1:1"), || {
        build_signing_keys(
            "testnet",
            Some(&account_id),
            &credentials_home,
            Some(&settings),
        )
    });
    let row = keys
        .iter()
        .find(|entry| entry.public_key == other_public_key)
        .expect("expected indexed signing key");
    assert!(!row.available_sources.contains(SOURCE_NEARXD_KEYCHAIN));
    assert!(!row.in_nearxd_keychain);
    assert!(!keychain_has_generic(
        KEYCHAIN_NEAR_CREDENTIAL_SERVICE,
        &nearxd_credential_account_key("testnet", &account_id, &other_public_key),
    ));

    let _ = keychain_delete_generic(KEYCHAIN_NEAR_CREDENTIAL_SERVICE, &legacy_account);
    let _ = keychain_delete_generic(
        KEYCHAIN_NEAR_CREDENTIAL_SERVICE,
        &nearxd_credential_account_key("testnet", &account_id, &mismatched_public_key),
    );
    let _ = keychain_delete_generic(
        KEYCHAIN_NEAR_CREDENTIAL_SERVICE,
        &nearxd_credential_account_key("testnet", &account_id, &other_public_key),
    );
    let _ = fs::remove_dir_all(&home);
}

#[cfg(target_os = "macos")]
#[test]
fn list_near_signing_keys_uses_near_cli_secure_fallback_when_rpc_unavailable() {
    if maybe_skip_keychain_integration_test(
        "list_near_signing_keys_uses_near_cli_secure_fallback_when_rpc_unavailable",
    ) {
        return;
    }
    let account_id = unique_test_account_id();
    let home = mktemp_dir("nearxd-near-cli-fallback-list");
    fs::write(
        home.join("accounts.json"),
        serde_json::to_string(&vec![json!({
            "account_id": account_id,
            "used_as_signer": true,
        })])
        .expect("encode accounts.json"),
    )
    .expect("write accounts.json");
    let (service, keychain_account, public_key, _) = match write_near_cli_secure_test_credential(
        "testnet",
        &account_id,
        "nearx-fallback-list-seed",
    ) {
        Ok(v) => v,
        Err(err) => {
            let _ = fs::remove_dir_all(&home);
            if maybe_skip_keychain_test(
                "list_near_signing_keys_uses_near_cli_secure_fallback_when_rpc_unavailable",
                &err,
            ) {
                return;
            }
            panic!("{err}");
        }
    };

    let state = Arc::new(BrokerState::default());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_env_var("NEAR_NODE_URL", Some("http://127.0.0.1:1"), || {
            let resp = handle_request(
                &state,
                BrokerRequest {
                    id: Some("x".to_string()),
                    method: "list_near_signing_keys".to_string(),
                    params: json!({
                        "network": "testnet",
                        "credentials_home_dir": home.display().to_string(),
                        "account_id": account_id,
                    }),
                },
            );
            assert!(resp.ok);
            let keys = resp
                .result
                .as_ref()
                .and_then(|v| v.get("keys"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let row = keys
                .iter()
                .find(|v| {
                    v.get("public_key")
                        .and_then(Value::as_str)
                        .map(|s| s == public_key)
                        .unwrap_or(false)
                })
                .expect("expected fallback-discovered key");
            assert_eq!(
                row.get("permission")
                    .and_then(|v| v.get("kind"))
                    .and_then(Value::as_str),
                Some("unknown")
            );
            let sources = row
                .get("available_sources")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            assert!(sources
                .iter()
                .any(|s| s.as_str() == Some(SOURCE_NEAR_CLI_SECURE)));
        });
    }));

    let _ = keychain_delete_generic(&service, &keychain_account);
    let _ = fs::remove_dir_all(home);
    if let Err(err) = result {
        std::panic::resume_unwind(err);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn import_near_signing_keys_uses_near_cli_secure_fallback_when_rpc_unavailable() {
    if maybe_skip_keychain_integration_test(
        "import_near_signing_keys_uses_near_cli_secure_fallback_when_rpc_unavailable",
    ) {
        return;
    }
    let account_id = unique_test_account_id();
    let home = mktemp_dir("nearxd-near-cli-fallback-import");
    fs::write(
        home.join("accounts.json"),
        serde_json::to_string(&vec![json!({
            "account_id": account_id,
            "used_as_signer": true,
        })])
        .expect("encode accounts.json"),
    )
    .expect("write accounts.json");
    let (service, keychain_account, public_key, _) = match write_near_cli_secure_test_credential(
        "testnet",
        &account_id,
        "nearx-fallback-import-seed",
    ) {
        Ok(v) => v,
        Err(err) => {
            let _ = fs::remove_dir_all(&home);
            if maybe_skip_keychain_test(
                "import_near_signing_keys_uses_near_cli_secure_fallback_when_rpc_unavailable",
                &err,
            ) {
                return;
            }
            panic!("{err}");
        }
    };

    let state = Arc::new(BrokerState::default());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_env_var("NEAR_NODE_URL", Some("http://127.0.0.1:1"), || {
            let resp = handle_request(
                &state,
                BrokerRequest {
                    id: Some("x".to_string()),
                    method: "import_near_signing_keys".to_string(),
                    params: json!({
                        "network": "testnet",
                        "credentials_home_dir": home.display().to_string(),
                        "account_id": account_id,
                        "sources": [SOURCE_NEAR_CLI_SECURE],
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
            let imported = resp
                .result
                .as_ref()
                .and_then(|v| v.get("imported"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            assert_eq!(
                imported
                    .first()
                    .and_then(|v| v.get("public_key"))
                    .and_then(Value::as_str),
                Some(public_key.as_str())
            );
            assert_eq!(
                imported
                    .first()
                    .and_then(|v| v.get("source"))
                    .and_then(Value::as_str),
                Some(SOURCE_NEAR_CLI_SECURE)
            );
            assert!(resp
                .result
                .as_ref()
                .and_then(|v| v.get("storage_backend"))
                .and_then(Value::as_str)
                .is_some());
            assert!(imported
                .first()
                .and_then(|v| v.get("storage_backend"))
                .and_then(Value::as_str)
                .is_some());
        });
    }));

    let _ = keychain_delete_generic(&service, &keychain_account);
    let _ = fs::remove_dir_all(home);
    if let Err(err) = result {
        std::panic::resume_unwind(err);
    }
}

#[test]
fn staking_watchlist_crud_persists_and_survives_reload() {
    let home = mktemp_dir("nearxd-staking-watchlist");
    let home_str = home.display().to_string();
    let account_a = unique_test_account_id();
    let ledger_path = "44'/397'/0'/0'/100'";
    let ledger_public_key = mock_ledger_get_public_key(ledger_path).to_string();
    let account_b = implicit_account_id_from_public_key(&mock_ledger_get_public_key(ledger_path));

    with_env_vars(
        &[
            ("HOME", Some(home_str.as_str())),
            ("NEAR_NODE_URL", Some("http://127.0.0.1:1")),
        ],
        || {
            let state = Arc::new(BrokerState::default());

            let initial = handle_request(
                &state,
                BrokerRequest {
                    id: Some("1".to_string()),
                    method: "list_staking_watchlist".to_string(),
                    params: json!({
                        "network": "testnet",
                    }),
                },
            );
            assert!(initial.ok);
            assert_eq!(
                initial
                    .result
                    .as_ref()
                    .and_then(|v| v.get("entries"))
                    .and_then(Value::as_array)
                    .map(Vec::len),
                Some(0)
            );

            let add_a = handle_request(
                &state,
                BrokerRequest {
                    id: Some("2".to_string()),
                    method: "add_staking_watchlist_account".to_string(),
                    params: json!({
                        "network": "testnet",
                        "account_id": account_a,
                        "source": "manual",
                        "prefer_keychain": false,
                    }),
                },
            );
            assert!(add_a.ok);

            let add_b = handle_request(
                &state,
                BrokerRequest {
                    id: Some("3".to_string()),
                    method: "add_staking_watchlist_account".to_string(),
                    params: json!({
                        "network": "testnet",
                        "account_id": account_b,
                        "source": "hardware_wallet",
                        "wallet_type": "ledger",
                        "public_key": ledger_public_key,
                        "derivation_path": ledger_path,
                        "prefer_keychain": false,
                    }),
                },
            );
            assert!(add_b.ok);

            // Simulate a fresh broker process by instantiating a new in-memory state.
            let state_reloaded = Arc::new(BrokerState::default());
            let after_reload = handle_request(
                &state_reloaded,
                BrokerRequest {
                    id: Some("4".to_string()),
                    method: "list_staking_watchlist".to_string(),
                    params: json!({
                        "network": "testnet",
                    }),
                },
            );
            assert!(after_reload.ok);
            let entries = after_reload
                .result
                .as_ref()
                .and_then(|v| v.get("entries"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            assert!(entries.iter().any(|v| {
                v.get("account_id")
                    .and_then(Value::as_str)
                    .map(|s| s == account_a)
                    .unwrap_or(false)
            }));
            assert!(entries.iter().any(|v| {
                v.get("account_id")
                    .and_then(Value::as_str)
                    .map(|s| s == account_b)
                    .unwrap_or(false)
            }));
            let hardware_entry = entries.iter().find(|v| {
                v.get("account_id")
                    .and_then(Value::as_str)
                    .map(|s| s == account_b)
                    .unwrap_or(false)
            });
            assert_eq!(
                hardware_entry
                    .and_then(|v| v.get("source"))
                    .and_then(Value::as_str),
                Some("hardware_wallet")
            );
            assert_eq!(
                hardware_entry
                    .and_then(|v| v.get("hardware_wallet"))
                    .and_then(|v| v.get("derivation_path"))
                    .and_then(Value::as_str),
                Some(ledger_path)
            );
            assert_eq!(
                hardware_entry
                    .and_then(|v| v.get("hardware_wallet"))
                    .and_then(|v| v.get("public_key"))
                    .and_then(Value::as_str),
                Some(ledger_public_key.as_str())
            );

            let removed = handle_request(
                &state_reloaded,
                BrokerRequest {
                    id: Some("5".to_string()),
                    method: "remove_staking_watchlist_account".to_string(),
                    params: json!({
                        "network": "testnet",
                        "account_id": account_a,
                        "prefer_keychain": false,
                    }),
                },
            );
            assert!(removed.ok);
            assert_eq!(
                removed
                    .result
                    .as_ref()
                    .and_then(|v| v.get("removed"))
                    .and_then(Value::as_bool),
                Some(true)
            );

            let state_final = Arc::new(BrokerState::default());
            let final_list = handle_request(
                &state_final,
                BrokerRequest {
                    id: Some("6".to_string()),
                    method: "list_staking_watchlist".to_string(),
                    params: json!({
                        "network": "testnet",
                    }),
                },
            );
            assert!(final_list.ok);
            let final_entries = final_list
                .result
                .as_ref()
                .and_then(|v| v.get("entries"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            assert!(!final_entries.iter().any(|v| {
                v.get("account_id")
                    .and_then(Value::as_str)
                    .map(|s| s == account_a)
                    .unwrap_or(false)
            }));
            assert!(final_entries.iter().any(|v| {
                v.get("account_id")
                    .and_then(Value::as_str)
                    .map(|s| s == account_b)
                    .unwrap_or(false)
            }));
        },
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn staking_watchlist_keeps_hardware_wallet_metadata_from_settings_json() {
    let derivation_path = "44'/397'/0'/0'/100'";
    let public_key = mock_ledger_get_public_key(derivation_path);
    let public_key_str = public_key.to_string();
    let account_id = implicit_account_id_from_public_key(&public_key);
    let settings = json!({
        STAKING_WATCHLIST_KEY: {
            "version": 1,
            "entries": [
                {
                    "network": "testnet",
                    "account_id": account_id,
                    "added_at_ms": 123,
                    "source": "hardware_wallet",
                    "hardware_wallet": {
                        "wallet_type": "ledger",
                        "public_key": public_key_str.clone(),
                        "derivation_path": derivation_path,
                    }
                }
            ]
        }
    });

    let entries = staking_watchlist_for_network(&settings, "testnet");
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.account_id, account_id);
    assert_eq!(entry.source, "hardware_wallet");
    assert_eq!(
        entry
            .hardware_wallet
            .as_ref()
            .map(|record| record.wallet_type.as_str()),
        Some("ledger")
    );
    assert_eq!(
        entry
            .hardware_wallet
            .as_ref()
            .map(|record| record.derivation_path.as_str()),
        Some(derivation_path)
    );
    assert_eq!(
        entry
            .hardware_wallet
            .as_ref()
            .map(|record| record.public_key.as_str()),
        Some(public_key_str.as_str())
    );
}

#[test]
fn add_staking_watchlist_account_rejects_invalid_account_id() {
    let state = Arc::new(BrokerState::default());
    let resp = handle_request(
        &state,
        BrokerRequest {
            id: Some("x".to_string()),
            method: "add_staking_watchlist_account".to_string(),
            params: json!({
                "network": "testnet",
                "account_id": "not a valid near account",
                "prefer_keychain": false,
            }),
        },
    );

    assert!(!resp.ok);
    assert_eq!(resp.error.as_ref().map(|e| e.code), Some("ERR_PARAMS"));
}

#[test]
fn get_signing_capabilities_reports_transport_and_secure_store_backend() {
    let state = Arc::new(BrokerState::default());
    let resp = handle_request(
        &state,
        BrokerRequest {
            id: Some("x".to_string()),
            method: "get_signing_capabilities".to_string(),
            params: json!({}),
        },
    );

    assert!(resp.ok);
    let result = resp.result.as_ref().expect("capabilities result");
    assert!(result.get("platform").and_then(Value::as_str).is_some());
    assert!(result
        .get("secure_store_backend")
        .and_then(Value::as_str)
        .is_some());
    assert!(result
        .get("supports_secure_store_persistence")
        .and_then(Value::as_bool)
        .is_some());
    #[cfg(windows)]
    assert_eq!(
        result.get("transport").and_then(Value::as_str),
        Some("named_pipe")
    );
    #[cfg(not(windows))]
    assert_eq!(
        result.get("transport").and_then(Value::as_str),
        Some("unix_socket")
    );
}

#[test]
fn list_near_signing_accounts_counts_near_cli_secure_source_as_has_keys() {
    let home = mktemp_dir("nearxd-signing-accounts-near-cli");
    let account_id = unique_test_account_id();
    fs::write(
        home.join("accounts.json"),
        serde_json::to_string(&vec![json!({
            "account_id": account_id,
            "used_as_signer": true,
        })])
        .expect("encode accounts.json"),
    )
    .expect("write accounts.json");

    let state = Arc::new(BrokerState::default());
    let resp = handle_request(
        &state,
        BrokerRequest {
            id: Some("x".to_string()),
            method: "list_near_signing_accounts".to_string(),
            params: json!({
                "network": "testnet",
                "credentials_home_dir": home.display().to_string(),
            }),
        },
    );

    assert!(resp.ok);
    let accounts = resp
        .result
        .as_ref()
        .and_then(|v| v.get("accounts"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let row = accounts
        .iter()
        .find(|entry| {
            entry
                .get("account_id")
                .and_then(Value::as_str)
                .map(|value| value == account_id)
                .unwrap_or(false)
        })
        .expect("expected near-cli account");
    assert_eq!(row.get("has_keys").and_then(Value::as_bool), Some(true));
    assert!(row
        .get("source_hints")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|value| value.as_str() == Some(SOURCE_NEAR_CLI_SECURE)));

    let _ = fs::remove_dir_all(home);
}

#[test]
fn signing_key_label_round_trips_and_survives_import_updates() {
    let home = mktemp_dir("nearxd-signing-label");
    let home_str = home.display().to_string();
    let credentials_home = home.join(".near-credentials");
    let network_dir = credentials_home.join("testnet");
    fs::create_dir_all(&network_dir).expect("create credentials dir");

    let account_id = unique_test_account_id();
    let secret = SecretKey::from_seed(near_crypto::KeyType::ED25519, "nearx-signing-label-seed");
    let public_key = secret.public_key().to_string();
    fs::write(
        network_dir.join(format!("{account_id}.json")),
        serde_json::to_string_pretty(&json!({
            "account_id": account_id,
            "public_key": public_key,
            "private_key": secret.to_string(),
        }))
        .expect("encode legacy credential"),
    )
    .expect("write legacy credential");

    with_env_vars(
        &[
            ("HOME", Some(home_str.as_str())),
            ("NEAR_NODE_URL", Some("http://127.0.0.1:1")),
        ],
        || {
            let state = Arc::new(BrokerState::default());

            let set_label = handle_request(
                &state,
                BrokerRequest {
                    id: Some("1".to_string()),
                    method: "set_signing_key_label".to_string(),
                    params: json!({
                        "network": "testnet",
                        "account_id": account_id,
                        "public_key": public_key,
                        "label": "Demo legacy",
                        "prefer_keychain": false,
                    }),
                },
            );
            assert!(set_label.ok);
            assert_eq!(
                set_label
                    .result
                    .as_ref()
                    .and_then(|v| v.get("label"))
                    .and_then(Value::as_str),
                Some("Demo legacy")
            );

            let import = handle_request(
                &state,
                BrokerRequest {
                    id: Some("2".to_string()),
                    method: "import_near_signing_keys".to_string(),
                    params: json!({
                        "network": "testnet",
                        "credentials_home_dir": credentials_home.display().to_string(),
                        "account_id": account_id,
                        "public_key": public_key,
                        "sources": ["legacy_file"],
                        "persist_in_keychain": false,
                        "save_settings": true,
                    }),
                },
            );
            assert!(
                import.ok,
                "expected import to succeed, got error: {:?}",
                import.error
            );

            let list = handle_request(
                &state,
                BrokerRequest {
                    id: Some("3".to_string()),
                    method: "list_near_signing_keys".to_string(),
                    params: json!({
                        "network": "testnet",
                        "credentials_home_dir": credentials_home.display().to_string(),
                        "account_id": account_id,
                    }),
                },
            );
            assert!(list.ok);

            let keys = list
                .result
                .as_ref()
                .and_then(|v| v.get("keys"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let row = keys
                .iter()
                .find(|entry| {
                    entry
                        .get("public_key")
                        .and_then(Value::as_str)
                        .map(|value| value == public_key)
                        .unwrap_or(false)
                })
                .expect("expected labeled signing key");
            assert_eq!(
                row.get("label").and_then(Value::as_str),
                Some("Demo legacy")
            );

            let settings_path = home.join(".nearx").join("signing_settings.json");
            let settings_raw = fs::read_to_string(settings_path).expect("read saved settings");
            let settings: Value =
                serde_json::from_str(&settings_raw).expect("parse saved settings");
            let records = settings
                .get(SIGNING_KEY_INDEX_KEY)
                .and_then(|v| v.get("records"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let indexed = records
                .iter()
                .find(|entry| {
                    entry
                        .get("public_key")
                        .and_then(Value::as_str)
                        .map(|value| value == public_key)
                        .unwrap_or(false)
                })
                .expect("expected indexed signing key");
            assert_eq!(
                indexed.get("label").and_then(Value::as_str),
                Some("Demo legacy")
            );
        },
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn build_signing_keys_does_not_report_deleted_keychain_source_from_index() {
    let home = mktemp_dir("nearxd-deleted-keychain-source");
    let credentials_home = home.join(".near-credentials");
    let network_dir = credentials_home.join("testnet");
    fs::create_dir_all(&network_dir).expect("create credentials dir");

    let account_id = unique_test_account_id();
    let secret = SecretKey::from_seed(
        near_crypto::KeyType::ED25519,
        "nearx-deleted-keychain-source",
    );
    let public_key = secret.public_key().to_string();
    fs::write(
        network_dir.join(format!("{account_id}.json")),
        serde_json::to_string_pretty(&json!({
            "account_id": account_id,
            "public_key": public_key,
            "private_key": secret.to_string(),
        }))
        .expect("encode legacy credential"),
    )
    .expect("write legacy credential");

    let settings = json!({
        SIGNING_KEY_INDEX_KEY: {
            "version": SIGNING_KEY_INDEX_VERSION,
            "records": [{
                "network": "testnet",
                "account_id": account_id,
                "public_key": public_key,
                "label": "Deleted keychain copy",
                "available_sources": [SOURCE_NEARXD_KEYCHAIN, SOURCE_LEGACY_FILE],
                "in_nearxd_keychain": true,
                "last_seen_at_ms": now_ms(),
            }],
        },
    });

    with_env_var("NEAR_NODE_URL", Some("http://127.0.0.1:1"), || {
        let keys = build_signing_keys(
            "testnet",
            Some(&account_id),
            &credentials_home,
            Some(&settings),
        );
        let row = keys
            .iter()
            .find(|entry| entry.public_key == public_key)
            .expect("expected signing key");
        assert_eq!(row.label.as_deref(), Some("Deleted keychain copy"));
        assert!(row.available_sources.contains(SOURCE_LEGACY_FILE));
        assert!(!row.available_sources.contains(SOURCE_NEARXD_KEYCHAIN));
        assert!(!row.in_nearxd_keychain);
    });

    let _ = fs::remove_dir_all(home);
}

#[test]
fn connect_hardware_wallet_and_sign_transaction_with_mock_adapter() {
    let home = mktemp_dir("nearxd-hardware-connect-sign");
    let home_str = home.display().to_string();
    let account_id = unique_test_account_id();
    let derivation_path = DEFAULT_LEDGER_DERIVATION_PATH.to_string();
    let public_key = mock_ledger_get_public_key(&derivation_path).to_string();
    let (rpc_url, rpc_handle) =
        spawn_mock_rpc_server(mock_view_access_key_list_response(&public_key));

    with_env_vars(
        &[
            ("HOME", Some(home_str.as_str())),
            ("NEARXD_HARDWARE_WALLET_ADAPTER", Some("mock")),
            ("NEAR_NODE_URL", Some(rpc_url.as_str())),
        ],
        || {
            let state = Arc::new(BrokerState::default());
            let connect = handle_request(
                &state,
                BrokerRequest {
                    id: Some("1".to_string()),
                    method: "connect_hardware_wallet".to_string(),
                    params: json!({
                        "network": "testnet",
                        "account_id": account_id,
                        "wallet_type": "ledger",
                        "derivation_path": derivation_path,
                        "display_confirm": false,
                        "prefer_keychain": false,
                        "credentials_home_dir": home_str,
                    }),
                },
            );
            assert!(connect.ok);
            assert_eq!(
                connect
                    .result
                    .as_ref()
                    .and_then(|v| v.get("account_binding"))
                    .and_then(Value::as_str),
                Some("selected_account")
            );
            assert_eq!(
                connect
                    .result
                    .as_ref()
                    .and_then(|v| v.get("public_key"))
                    .and_then(Value::as_str),
                Some(public_key.as_str())
            );
            let sources = connect
                .result
                .as_ref()
                .and_then(|v| v.get("available_sources"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            assert!(sources
                .iter()
                .any(|s| s.as_str() == Some(SOURCE_HARDWARE_WALLET)));
            assert!(connect
                .result
                .as_ref()
                .and_then(|v| v.get("storage_backend"))
                .and_then(Value::as_str)
                .is_some());

            let sign = handle_request(
                &state,
                BrokerRequest {
                    id: Some("2".to_string()),
                    method: "sign_transaction".to_string(),
                    params: json!({
                        "signer_id": account_id,
                        "signer_public_key": public_key,
                        "credential_source": SOURCE_HARDWARE_WALLET,
                        "receiver_id": "receiver.testnet",
                        "nonce": 42,
                        "block_hash": "11111111111111111111111111111111",
                        "actions": [{ "type": "Transfer", "deposit": "1" }],
                        "network": "testnet",
                        "reason": "test hardware wallet sign path",
                    }),
                },
            );
            assert!(sign.ok);
            assert_eq!(
                sign.result
                    .as_ref()
                    .and_then(|v| v.get("credential_source"))
                    .and_then(Value::as_str),
                Some(SOURCE_HARDWARE_WALLET)
            );
            assert_eq!(
                sign.result
                    .as_ref()
                    .and_then(|v| v.get("public_key"))
                    .and_then(Value::as_str),
                Some(public_key.as_str())
            );
        },
    );

    let _ = rpc_handle.join();
    let _ = fs::remove_dir_all(home);
}

#[test]
fn connect_hardware_wallet_without_account_uses_implicit_account() {
    let home = mktemp_dir("nearxd-hardware-connect-implicit");
    let home_str = home.display().to_string();
    let derivation_path = "44'/397'/0'/0'/100'".to_string();
    let public_key = mock_ledger_get_public_key(&derivation_path);
    let public_key_str = public_key.to_string();
    let implicit_account_id = implicit_account_id_from_public_key(&public_key);

    with_env_vars(
        &[
            ("HOME", Some(home_str.as_str())),
            ("NEARXD_HARDWARE_WALLET_ADAPTER", Some("mock")),
            ("NEAR_NODE_URL", Some("http://127.0.0.1:1")),
        ],
        || {
            let state = Arc::new(BrokerState::default());
            let connect = handle_request(
                &state,
                BrokerRequest {
                    id: Some("implicit-1".to_string()),
                    method: "connect_hardware_wallet".to_string(),
                    params: json!({
                        "network": "testnet",
                        "wallet_type": "ledger",
                        "derivation_path": derivation_path,
                        "display_confirm": false,
                        "prefer_keychain": false,
                        "credentials_home_dir": home_str,
                    }),
                },
            );
            assert!(connect.ok);
            assert_eq!(
                connect
                    .result
                    .as_ref()
                    .and_then(|v| v.get("account_binding"))
                    .and_then(Value::as_str),
                Some("implicit_account")
            );
            assert_eq!(
                connect
                    .result
                    .as_ref()
                    .and_then(|v| v.get("requested_account_id"))
                    .and_then(Value::as_str),
                None
            );
            assert_eq!(
                connect
                    .result
                    .as_ref()
                    .and_then(|v| v.get("implicit_account_id"))
                    .and_then(Value::as_str),
                Some(implicit_account_id.as_str())
            );
            assert_eq!(
                connect
                    .result
                    .as_ref()
                    .and_then(|v| v.get("account_id"))
                    .and_then(Value::as_str),
                Some(implicit_account_id.as_str())
            );
            assert_eq!(
                connect
                    .result
                    .as_ref()
                    .and_then(|v| v.get("public_key"))
                    .and_then(Value::as_str),
                Some(public_key_str.as_str())
            );

            let accounts = handle_request(
                &state,
                BrokerRequest {
                    id: Some("implicit-list-accounts".to_string()),
                    method: "list_near_signing_accounts".to_string(),
                    params: json!({
                        "network": "testnet",
                        "credentials_home_dir": home_str,
                    }),
                },
            );
            assert!(accounts.ok);
            let listed_account = accounts
                .result
                .as_ref()
                .and_then(|v| v.get("accounts"))
                .and_then(Value::as_array)
                .and_then(|entries| {
                    entries.iter().find(|entry| {
                        entry
                            .get("account_id")
                            .and_then(Value::as_str)
                            .map(|value| value == implicit_account_id)
                            .unwrap_or(false)
                    })
                });
            assert_eq!(
                listed_account
                    .and_then(|v| v.get("hardware_wallets"))
                    .and_then(Value::as_array)
                    .and_then(|entries| entries.first())
                    .and_then(|v| v.get("derivation_path"))
                    .and_then(Value::as_str),
                Some(derivation_path.as_str())
            );

            let keys = handle_request(
                &state,
                BrokerRequest {
                    id: Some("implicit-list-keys".to_string()),
                    method: "list_near_signing_keys".to_string(),
                    params: json!({
                        "network": "testnet",
                        "account_id": implicit_account_id,
                        "credentials_home_dir": home_str,
                    }),
                },
            );
            assert!(keys.ok);
            let listed_key = keys
                .result
                .as_ref()
                .and_then(|v| v.get("keys"))
                .and_then(Value::as_array)
                .and_then(|entries| entries.first());
            assert_eq!(
                listed_key
                    .and_then(|v| v.get("hardware_wallet"))
                    .and_then(|v| v.get("derivation_path"))
                    .and_then(Value::as_str),
                Some(derivation_path.as_str())
            );
            assert_eq!(
                listed_key
                    .and_then(|v| v.get("hardware_wallet"))
                    .and_then(|v| v.get("public_key"))
                    .and_then(Value::as_str),
                Some(public_key_str.as_str())
            );

            let sign = handle_request(
                &state,
                BrokerRequest {
                    id: Some("implicit-2".to_string()),
                    method: "sign_transaction".to_string(),
                    params: json!({
                        "signer_id": implicit_account_id,
                        "signer_public_key": public_key_str,
                        "credential_source": SOURCE_HARDWARE_WALLET,
                        "receiver_id": "receiver.testnet",
                        "nonce": 7,
                        "block_hash": "11111111111111111111111111111111",
                        "actions": [{ "type": "Transfer", "deposit": "1" }],
                        "network": "testnet",
                        "reason": "test implicit hardware wallet sign path",
                    }),
                },
            );
            assert!(sign.ok);
            assert_eq!(
                sign.result
                    .as_ref()
                    .and_then(|v| v.get("credential_source"))
                    .and_then(Value::as_str),
                Some(SOURCE_HARDWARE_WALLET)
            );
        },
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn connect_hardware_wallet_rejects_key_not_on_account() {
    let home = mktemp_dir("nearxd-hardware-connect-mismatch");
    let home_str = home.display().to_string();
    let account_id = unique_test_account_id();
    let derivation_path = DEFAULT_LEDGER_DERIVATION_PATH.to_string();
    let other_key = SecretKey::from_seed(near_crypto::KeyType::ED25519, "nearx-not-on-account")
        .public_key()
        .to_string();
    let (rpc_url, rpc_handle) =
        spawn_mock_rpc_server(mock_view_access_key_list_response(&other_key));

    with_env_vars(
        &[
            ("HOME", Some(home_str.as_str())),
            ("NEARXD_HARDWARE_WALLET_ADAPTER", Some("mock")),
            ("NEAR_NODE_URL", Some(rpc_url.as_str())),
        ],
        || {
            let state = Arc::new(BrokerState::default());
            let connect = handle_request(
                &state,
                BrokerRequest {
                    id: Some("x".to_string()),
                    method: "connect_hardware_wallet".to_string(),
                    params: json!({
                        "network": "testnet",
                        "account_id": account_id,
                        "wallet_type": "ledger",
                        "derivation_path": derivation_path,
                        "display_confirm": false,
                        "prefer_keychain": false,
                        "credentials_home_dir": home_str,
                    }),
                },
            );
            assert!(!connect.ok);
            assert_eq!(
                connect.error.as_ref().map(|e| e.code),
                Some("ERR_HARDWARE_KEY_NOT_ON_ACCOUNT")
            );
        },
    );

    let _ = rpc_handle.join();
    let _ = fs::remove_dir_all(home);
}

#[test]
fn sign_transaction_hardware_unavailable_returns_coded_error() {
    let home = mktemp_dir("nearxd-hardware-unavailable");
    let home_str = home.display().to_string();
    let account_id = unique_test_account_id();
    let public_key = mock_ledger_get_public_key(DEFAULT_LEDGER_DERIVATION_PATH).to_string();

    with_env_vars(
        &[
            ("HOME", Some(home_str.as_str())),
            ("NEARXD_HARDWARE_WALLET_ADAPTER", Some("none")),
        ],
        || {
            let settings = json!({
                HARDWARE_WALLET_INDEX_KEY: {
                    "version": HARDWARE_WALLET_INDEX_VERSION,
                    "records": [
                        {
                            "network": "testnet",
                            "account_id": account_id,
                            "public_key": public_key,
                            "wallet_type": HARDWARE_WALLET_TYPE_LEDGER,
                            "derivation_path": DEFAULT_LEDGER_DERIVATION_PATH,
                            "last_seen_at_ms": now_ms()
                        }
                    ]
                }
            });
            write_signing_settings_file(&settings).expect("write test signing settings");

            let state = Arc::new(BrokerState::default());
            let sign = handle_request(
                &state,
                BrokerRequest {
                    id: Some("x".to_string()),
                    method: "sign_transaction".to_string(),
                    params: json!({
                        "signer_id": account_id,
                        "signer_public_key": public_key,
                        "credential_source": SOURCE_HARDWARE_WALLET,
                        "receiver_id": "receiver.testnet",
                        "nonce": 9,
                        "block_hash": "11111111111111111111111111111111",
                        "actions": [{ "type": "Transfer", "deposit": "1" }],
                        "network": "testnet",
                    }),
                },
            );
            assert!(!sign.ok);
            assert_eq!(sign.error.as_ref().map(|e| e.code), Some("ERR_UNAVAILABLE"));
        },
    );

    let _ = fs::remove_dir_all(home);
}

#[test]
fn signing_key_index_marks_stale_and_prunes_very_old_records() {
    let home = mktemp_dir("nearxd-index-stale-prune");
    let now = now_ms();
    let fresh_key = "ed25519:fresh".to_string();
    let stale_key = "ed25519:stale".to_string();
    let prunable_key = "ed25519:old".to_string();
    let settings = json!({
        SIGNING_KEY_INDEX_KEY: {
            "version": SIGNING_KEY_INDEX_VERSION,
            "records": [
                {
                    "network": "testnet",
                    "account_id": "fresh.testnet",
                    "public_key": fresh_key,
                    "available_sources": [SOURCE_NEARXD_KEYCHAIN],
                    "in_nearxd_keychain": true,
                    "last_seen_at_ms": now.saturating_sub(1_000),
                },
                {
                    "network": "testnet",
                    "account_id": "stale.testnet",
                    "public_key": stale_key,
                    "available_sources": [SOURCE_NEARXD_KEYCHAIN],
                    "in_nearxd_keychain": true,
                    "last_seen_at_ms": now.saturating_sub(SIGNING_KEY_STALE_AFTER_MS + 1_000),
                },
                {
                    "network": "testnet",
                    "account_id": "old.testnet",
                    "public_key": prunable_key,
                    "available_sources": [SOURCE_NEARXD_KEYCHAIN],
                    "in_nearxd_keychain": true,
                    "last_seen_at_ms": now.saturating_sub(SIGNING_KEY_PRUNE_AFTER_MS + 1_000),
                }
            ]
        }
    });

    let keys = build_signing_keys("testnet", None, &home, Some(&settings));
    assert!(keys
        .iter()
        .any(|k| k.public_key == "ed25519:fresh" && !k.stale));
    assert!(keys
        .iter()
        .any(|k| k.public_key == "ed25519:stale" && k.stale));
    assert!(!keys.iter().any(|k| k.public_key == "ed25519:old"));
    let _ = fs::remove_dir_all(home);
}

#[cfg(target_os = "macos")]
#[test]
fn sign_transaction_supports_secp256k1_function_call_fixture() {
    if maybe_skip_keychain_integration_test(
        "sign_transaction_supports_secp256k1_function_call_fixture",
    ) {
        return;
    }
    use borsh::BorshDeserialize;
    let account_id = unique_test_account_id();
    let (service, keychain_account, public_key, _) =
        match write_near_cli_secure_test_credential_with_key_type(
            "testnet",
            &account_id,
            near_crypto::KeyType::SECP256K1,
            "nearx-secp-function-call-seed",
        ) {
            Ok(v) => v,
            Err(err) => {
                if maybe_skip_keychain_test(
                    "sign_transaction_supports_secp256k1_function_call_fixture",
                    &err,
                ) {
                    return;
                }
                panic!("{err}");
            }
        };

    let state = Arc::new(BrokerState::default());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let resp = handle_request(
            &state,
            BrokerRequest {
                id: Some("x".to_string()),
                method: "sign_transaction".to_string(),
                params: json!({
                    "signer_id": account_id,
                    "signer_public_key": public_key,
                    "credential_source": "near_cli_secure",
                    "receiver_id": "receiver.testnet",
                    "nonce": 7,
                    "block_hash": "11111111111111111111111111111111",
                    "actions": [{
                        "type": "FunctionCall",
                        "method_name": "set_status",
                        "args": "e30=",
                        "gas": 30000000000000u64,
                        "deposit": "0"
                    }],
                    "network": "testnet",
                    "reason": "test secp256k1 function-call signing fixture",
                }),
            },
        );
        assert!(resp.ok);
        let result = resp.result.as_ref().expect("sign result");
        assert_eq!(
            result.get("public_key").and_then(Value::as_str),
            Some(public_key.as_str())
        );
        assert_eq!(
            result.get("credential_source").and_then(Value::as_str),
            Some(SOURCE_NEAR_CLI_SECURE)
        );

        let signed_b64 = result
            .get("signed_transaction_base64")
            .and_then(Value::as_str)
            .expect("signed tx b64");
        let signed_bytes = STANDARD.decode(signed_b64).expect("decode signed tx b64");
        let signed_tx = SignedTransaction::try_from_slice(&signed_bytes)
            .expect("borsh decode signed transaction");
        let (tx_public_key, tx_actions) = match &signed_tx.transaction {
            Transaction::V0(v0) => (v0.public_key.to_string(), &v0.actions),
            Transaction::V1(v1) => (v1.public_key.to_string(), &v1.actions),
        };
        assert_eq!(tx_public_key, public_key);
        assert_eq!(tx_actions.len(), 1);
        match &tx_actions[0] {
            Action::FunctionCall(fc) => {
                assert_eq!(fc.method_name, "set_status");
                assert_eq!(fc.deposit, 0);
                assert_eq!(fc.gas, 30_000_000_000_000);
            }
            _ => panic!("expected FunctionCall action"),
        }
    }));

    let _ = keychain_delete_generic(&service, &keychain_account);
    if let Err(err) = result {
        std::panic::resume_unwind(err);
    }
}

// ── parse_near_actions tests ────────────────────────────────────────

#[test]
fn parse_near_actions_transfer() {
    use crate::broker::parse_near_actions;
    let actions = parse_near_actions(&[json!({
        "type": "Transfer",
        "deposit": "1000000000000000000000000"
    })])
    .unwrap();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::Transfer(_)));
}

#[test]
fn parse_near_actions_function_call() {
    use crate::broker::parse_near_actions;
    let actions = parse_near_actions(&[json!({
        "type": "FunctionCall",
        "method_name": "ft_transfer",
        "args": "e30=",
        "gas": 30000000000000_u64,
        "deposit": "1"
    })])
    .unwrap();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::FunctionCall(_)));
}

#[test]
fn parse_near_actions_create_account() {
    use crate::broker::parse_near_actions;
    let actions = parse_near_actions(&[json!({
        "type": "CreateAccount"
    })])
    .unwrap();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::CreateAccount(_)));
}

#[test]
fn parse_near_actions_delete_account() {
    use crate::broker::parse_near_actions;
    let actions = parse_near_actions(&[json!({
        "type": "DeleteAccount",
        "beneficiary_id": "bob.near"
    })])
    .unwrap();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::DeleteAccount(_)));
}

#[test]
fn parse_near_actions_delete_account_missing_beneficiary() {
    use crate::broker::parse_near_actions;
    let err = parse_near_actions(&[json!({
        "type": "DeleteAccount"
    })])
    .unwrap_err();
    assert!(err.contains("beneficiary_id"), "error: {err}");
}

#[test]
fn parse_near_actions_delete_key() {
    use crate::broker::parse_near_actions;
    let actions = parse_near_actions(&[json!({
        "type": "DeleteKey",
        "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp"
    })])
    .unwrap();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::DeleteKey(_)));
}

#[test]
fn parse_near_actions_delete_key_missing_pk() {
    use crate::broker::parse_near_actions;
    let err = parse_near_actions(&[json!({
        "type": "DeleteKey"
    })])
    .unwrap_err();
    assert!(err.contains("public_key"), "error: {err}");
}

#[test]
fn parse_near_actions_add_key_full_access() {
    use crate::broker::parse_near_actions;
    let actions = parse_near_actions(&[json!({
        "type": "AddKey",
        "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp",
        "permission": "FullAccess"
    })])
    .unwrap();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::AddKey(_)));
}

#[test]
fn parse_near_actions_add_key_function_call() {
    use crate::broker::parse_near_actions;
    let actions = parse_near_actions(&[json!({
        "type": "AddKey",
        "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp",
        "permission": {
            "type": "FunctionCall",
            "allowance": "250000000000000000000000",
            "receiver_id": "contract.near",
            "method_names": ["method1", "method2"]
        }
    })])
    .unwrap();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::AddKey(_)));
}

#[test]
fn parse_near_actions_add_key_function_call_null_allowance() {
    use crate::broker::parse_near_actions;
    let actions = parse_near_actions(&[json!({
        "type": "AddKey",
        "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp",
        "permission": {
            "type": "FunctionCall",
            "allowance": null,
            "receiver_id": "contract.near"
        }
    })])
    .unwrap();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::AddKey(_)));
}

#[test]
fn parse_near_actions_add_key_missing_permission() {
    use crate::broker::parse_near_actions;
    let err = parse_near_actions(&[json!({
        "type": "AddKey",
        "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp"
    })])
    .unwrap_err();
    assert!(err.contains("permission"), "error: {err}");
}

#[test]
fn parse_near_actions_add_key_missing_receiver() {
    use crate::broker::parse_near_actions;
    let err = parse_near_actions(&[json!({
        "type": "AddKey",
        "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp",
        "permission": {
            "type": "FunctionCall"
        }
    })])
    .unwrap_err();
    assert!(err.contains("receiver_id"), "error: {err}");
}

#[test]
fn parse_near_actions_stake() {
    use crate::broker::parse_near_actions;
    let actions = parse_near_actions(&[json!({
        "type": "Stake",
        "stake": "1000000000000000000000000",
        "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp"
    })])
    .unwrap();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::Stake(_)));
}

#[test]
fn parse_near_actions_stake_missing_fields() {
    use crate::broker::parse_near_actions;
    let err = parse_near_actions(&[json!({
        "type": "Stake",
        "stake": "1000"
    })])
    .unwrap_err();
    assert!(err.contains("public_key"), "error: {err}");

    let err = parse_near_actions(&[json!({
        "type": "Stake",
        "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp"
    })])
    .unwrap_err();
    assert!(err.contains("stake"), "error: {err}");
}

#[test]
fn parse_near_actions_unsupported_type() {
    use crate::broker::parse_near_actions;
    let err = parse_near_actions(&[json!({
        "type": "Delegate",
        "actions": []
    })])
    .unwrap_err();
    assert!(err.contains("unsupported"), "error: {err}");
    assert!(err.contains("Delegate"), "error: {err}");
}

#[test]
fn parse_near_actions_deploy_contract() {
    use crate::broker::parse_near_actions;
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let wasm_bytes = vec![0x00, 0x61, 0x73, 0x6d]; // WASM magic
    let code_b64 = STANDARD.encode(&wasm_bytes);
    let actions = parse_near_actions(&[json!({
        "type": "DeployContract",
        "code": code_b64,
    })])
    .unwrap();
    assert_eq!(actions.len(), 1);
}

#[test]
fn parse_near_actions_deploy_contract_missing_code() {
    use crate::broker::parse_near_actions;
    let err = parse_near_actions(&[json!({
        "type": "DeployContract"
    })])
    .unwrap_err();
    assert!(err.contains("code"), "error: {err}");
}

#[test]
fn parse_near_actions_deploy_contract_invalid_base64() {
    use crate::broker::parse_near_actions;
    let err = parse_near_actions(&[json!({
        "type": "DeployContract",
        "code": "not!!valid!!base64"
    })])
    .unwrap_err();
    assert!(err.contains("base64"), "error: {err}");
}

// ---- sign_transaction: sender==receiver validation for self-targeting actions ----

/// Helper: build a sign_transaction request with given signer, receiver, and actions.
fn sign_tx_request(signer: &str, receiver: &str, actions: Value) -> BrokerRequest {
    BrokerRequest {
        id: Some("test".into()),
        method: "sign_transaction".into(),
        params: json!({
            "signer_id": signer,
            "receiver_id": receiver,
            "nonce": 1,
            "block_hash": "11111111111111111111111111111111",
            "actions": actions,
            "network": "testnet",
        }),
    }
}

/// Self-targeting actions must be rejected when receiver != signer.
#[test]
fn sign_transaction_rejects_self_targeting_action_with_wrong_receiver() {
    let state = Arc::new(BrokerState::new());

    let self_targeting_actions = vec![
        json!([{"type": "DeployContract", "code": "AAAA"}]),
        json!([{"type": "Stake", "stake": "1000", "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp"}]),
        json!([{"type": "AddKey", "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp", "permission": "FullAccess"}]),
        json!([{"type": "DeleteKey", "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp"}]),
        json!([{"type": "DeleteAccount", "beneficiary_id": "bob.testnet"}]),
    ];
    let expected_names = ["DeployContract", "Stake", "AddKey", "DeleteKey", "DeleteAccount"];

    for (actions, expected_name) in self_targeting_actions.iter().zip(expected_names.iter()) {
        let req = sign_tx_request("alice.testnet", "bob.testnet", actions.clone());
        let resp = handle_request(&state, req);
        let err = resp.error.as_ref().unwrap_or_else(|| {
            panic!("{expected_name}: expected error when receiver != signer")
        });
        assert!(
            err.message.contains("requires receiver_id to equal signer_id"),
            "{expected_name}: unexpected error message: {}", err.message
        );
        assert!(
            err.message.contains(expected_name),
            "{expected_name}: error should name the action type: {}", err.message
        );
    }
}

/// Non-self-targeting actions should NOT be rejected for receiver != signer
/// (they will fail later for missing credentials, not for the receiver check).
#[test]
fn sign_transaction_allows_different_receiver_for_transfer() {
    let state = Arc::new(BrokerState::new());
    let req = sign_tx_request(
        "alice.testnet",
        "bob.testnet",
        json!([{"type": "Transfer", "deposit": "1000000000000000000000000"}]),
    );
    let resp = handle_request(&state, req);
    // Should fail downstream (credential resolution), not at the receiver check.
    let err = resp.error.as_ref().expect("expected error (no credentials)");
    assert!(
        !err.message.contains("requires receiver_id to equal signer_id"),
        "Transfer should not trigger self-targeting check: {}", err.message
    );
}

/// Mixed actions: a self-targeting action in a batch with non-self-targeting should still be caught.
#[test]
fn sign_transaction_rejects_mixed_batch_with_self_targeting_action() {
    let state = Arc::new(BrokerState::new());
    let req = sign_tx_request(
        "alice.testnet",
        "bob.testnet",
        json!([
            {"type": "Transfer", "deposit": "1000"},
            {"type": "AddKey", "public_key": "ed25519:6E8sCci9badyRkXb3JoRpBj5p8C6Tw41ELDZoiihKEtp", "permission": "FullAccess"}
        ]),
    );
    let resp = handle_request(&state, req);
    let err = resp.error.as_ref().expect("expected error for mixed batch");
    assert!(
        err.message.contains("action[1]") && err.message.contains("AddKey"),
        "should identify action[1] AddKey: {}", err.message
    );
}
