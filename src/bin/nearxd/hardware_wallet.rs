use near_crypto::PublicKey;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::env;
use std::fmt::Write as _;
use std::sync::Mutex;

use crate::credentials::{
    collect_legacy_credentials, credential_curve_type, near_cli_secure_has_credential,
    nearxd_keychain_has_scoped_credential, SOURCE_HARDWARE_WALLET, SOURCE_LEGACY_FILE,
    SOURCE_NEARXD_KEYCHAIN, SOURCE_NEAR_CLI_SECURE,
};
use crate::rpc::{access_key_permission_to_summary, fetch_onchain_access_keys};
use crate::settings::{
    hardware_wallet_record_for_key, load_signing_settings, persist_signing_settings,
    signing_key_label, upsert_hardware_wallet_index, upsert_signing_key_index,
    HardwareWalletIndexRecord, IndexedSigningKeyRecord,
};
use crate::signing::{ordered_sources_from_set, preferred_source_from_set};
use crate::util::now_ms;

pub(crate) const HARDWARE_WALLET_TYPE_LEDGER: &str = "ledger";
pub(crate) const DEFAULT_LEDGER_DERIVATION_PATH: &str = "44'/397'/0'/0'/1'";

#[derive(Debug, Clone)]
pub(crate) struct HardwareWalletError {
    pub code: &'static str,
    pub message: String,
}

impl HardwareWalletError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

fn hardware_wallet_adapter() -> String {
    env::var("NEARXD_HARDWARE_WALLET_ADAPTER")
        .unwrap_or_else(|_| "auto".to_string())
        .trim()
        .to_ascii_lowercase()
}

pub(crate) fn parse_ledger_derivation_path(
    derivation_path: Option<&str>,
) -> Result<slipped10::BIP32Path, HardwareWalletError> {
    let path = derivation_path
        .unwrap_or(DEFAULT_LEDGER_DERIVATION_PATH)
        .trim();
    if path.is_empty() {
        return Err(HardwareWalletError::new(
            "ERR_HARDWARE_INVALID_PATH",
            "derivation_path cannot be empty",
        ));
    }
    path.parse::<slipped10::BIP32Path>().map_err(|e| {
        HardwareWalletError::new(
            "ERR_HARDWARE_INVALID_PATH",
            format!("invalid derivation_path '{path}': {e}"),
        )
    })
}

pub(crate) fn mock_ledger_secret_key(path: &str) -> near_crypto::SecretKey {
    near_crypto::SecretKey::from_seed(
        near_crypto::KeyType::ED25519,
        &format!("nearxd-ledger-mock-{path}"),
    )
}

pub(crate) fn mock_ledger_get_public_key(path: &str) -> PublicKey {
    mock_ledger_secret_key(path).public_key()
}

fn implicit_account_id_from_public_key(
    public_key: &PublicKey,
) -> Result<String, HardwareWalletError> {
    let key_type = public_key.key_type();
    let PublicKey::ED25519(_) = public_key else {
        return Err(HardwareWalletError::new(
            "ERR_HARDWARE_UNSUPPORTED_KEY_TYPE",
            format!("implicit account derivation requires an ed25519 public key, got {key_type}"),
        ));
    };

    let mut account_id = String::with_capacity(public_key.key_data().len() * 2);
    for byte in public_key.key_data() {
        let _ = write!(&mut account_id, "{byte:02x}");
    }
    Ok(account_id)
}

fn mock_ledger_sign_transaction(
    unsigned_tx: &[u8],
    path: &str,
) -> Result<near_crypto::Signature, HardwareWalletError> {
    use borsh::BorshDeserialize;
    use near_primitives::transaction::{Transaction, TransactionV0};

    let tx_v0 = TransactionV0::try_from_slice(unsigned_tx).map_err(|e| {
        HardwareWalletError::new(
            "ERR_HARDWARE_TRANSPORT",
            format!("decode unsigned transaction for mock signer failed: {e}"),
        )
    })?;
    let (tx_hash, _size) = Transaction::V0(tx_v0).get_hash_and_size();
    Ok(mock_ledger_secret_key(path).sign(tx_hash.as_ref()))
}

// Ledger APDU constants and functions for native desktop targets.

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
const LEDGER_CLA: u8 = 0x80;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
const LEDGER_INS_GET_PUBLIC_KEY: u8 = 0x04;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
const LEDGER_INS_SIGN_TRANSACTION: u8 = 0x02;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
const LEDGER_INS_GET_VERSION: u8 = 0x06;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
const LEDGER_RETURN_CODE_OK: u16 = 0x9000;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
const LEDGER_CHUNK_SIZE: usize = 250;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
const LEDGER_NETWORK_ID: u8 = b'W';
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
const LEDGER_P1_GET_PUBLIC_KEY_DISPLAY: u8 = 0x00;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
const LEDGER_P1_GET_PUBLIC_KEY_SILENT: u8 = 0x01;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
const LEDGER_P1_SIGN_CHUNK: u8 = 0x00;
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
const LEDGER_P1_SIGN_LAST_CHUNK: u8 = 0x80;

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn map_ledger_retcode(retcode: u16) -> HardwareWalletError {
    match retcode {
        0x5501 => HardwareWalletError::new(
            "ERR_HARDWARE_USER_REJECTED",
            "ledger user rejected the request",
        ),
        0x6807 | 0x6D00 | 0x6E00 => HardwareWalletError::new(
            "ERR_HARDWARE_APP_NOT_OPEN",
            "NEAR app is not open on Ledger",
        ),
        0x670A => HardwareWalletError::new(
            "ERR_HARDWARE_INVALID_PATH",
            "Ledger rejected the derivation path",
        ),
        0x5515 => HardwareWalletError::new(
            "ERR_HARDWARE_UNAVAILABLE",
            "Ledger is locked or unavailable",
        ),
        0x6990 => HardwareWalletError::new(
            "ERR_HARDWARE_TRANSPORT",
            "Ledger APDU error 0x6990 — the NEAR app may need updating. \
             Install the latest NEAR app via Ledger Live.",
        ),
        _ => HardwareWalletError::new(
            "ERR_HARDWARE_TRANSPORT",
            format!("ledger APDU failed with status 0x{retcode:04X}"),
        ),
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn map_ledger_transport_error(err: impl ToString) -> HardwareWalletError {
    let msg = err.to_string();
    let lower = msg.to_ascii_lowercase();
    if lower.contains("device not found")
        || lower.contains("no such device")
        || lower.contains("not connected")
    {
        HardwareWalletError::new("ERR_HARDWARE_UNAVAILABLE", msg)
    } else {
        HardwareWalletError::new("ERR_HARDWARE_TRANSPORT", msg)
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn ledger_hd_path_to_bytes(hd_path: &slipped10::BIP32Path) -> Vec<u8> {
    (0..hd_path.depth())
        .flat_map(|index| hd_path.index(index).unwrap().to_be_bytes())
        .collect::<Vec<u8>>()
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn ledger_get_transport() -> Result<ledger_transport_hid::TransportNativeHID, HardwareWalletError> {
    use ledger_transport_hid::hidapi::{HidApi, HidError};
    use ledger_transport_hid::LedgerHIDError;

    let hidapi = HidApi::new().map_err(|e: HidError| map_ledger_transport_error(e))?;
    ledger_transport_hid::TransportNativeHID::new(&hidapi)
        .map_err(|e: LedgerHIDError| map_ledger_transport_error(format!("{e:?}")))
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn ledger_exchange(
    command: &ledger_transport::APDUCommand<Vec<u8>>,
) -> Result<ledger_apdu::APDUAnswer<Vec<u8>>, HardwareWalletError> {
    use ledger_transport_hid::LedgerHIDError;

    let transport = ledger_get_transport()?;
    transport
        .exchange(command)
        .map_err(|e: LedgerHIDError| map_ledger_transport_error(format!("{e:?}")))
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn ledger_running_app_name() -> Result<String, HardwareWalletError> {
    let command = ledger_transport::APDUCommand {
        cla: 0xB0,
        ins: 0x01,
        p1: 0x00,
        p2: 0x00,
        data: vec![],
    };
    let response = ledger_exchange(&command)?;
    if response.retcode() != LEDGER_RETURN_CODE_OK {
        return Err(map_ledger_retcode(response.retcode()));
    }
    let data = response.data();
    if data.len() < 2 {
        return Err(HardwareWalletError::new(
            "ERR_HARDWARE_TRANSPORT",
            "ledger app-name response is too short",
        ));
    }
    let name_len = data[1] as usize;
    if data.len() < 2 + name_len {
        return Err(HardwareWalletError::new(
            "ERR_HARDWARE_TRANSPORT",
            "ledger app-name response length is invalid",
        ));
    }
    Ok(String::from_utf8_lossy(&data[2..2 + name_len]).to_string())
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn ledger_quit_current_app() -> Result<(), HardwareWalletError> {
    let command = ledger_transport::APDUCommand {
        cla: 0xB0,
        ins: 0xA7,
        p1: 0x00,
        p2: 0x00,
        data: vec![],
    };
    let response = ledger_exchange(&command)?;
    if response.retcode() == LEDGER_RETURN_CODE_OK {
        Ok(())
    } else {
        Err(map_ledger_retcode(response.retcode()))
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn ledger_open_near_app() -> Result<(), HardwareWalletError> {
    match ledger_running_app_name()?.as_str() {
        "NEAR" => return Ok(()),
        "BOLOS" => {}
        _ => {
            ledger_quit_current_app()?;
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    let command = ledger_transport::APDUCommand {
        cla: 0xE0,
        ins: 0xD8,
        p1: 0x00,
        p2: 0x00,
        data: vec![b'N', b'E', b'A', b'R'],
    };
    let response = ledger_exchange(&command)?;
    if response.retcode() == LEDGER_RETURN_CODE_OK {
        Ok(())
    } else {
        Err(map_ledger_retcode(response.retcode()))
    }
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn ledger_get_public_key(
    derivation_path: &slipped10::BIP32Path,
    display_confirm: bool,
) -> Result<PublicKey, HardwareWalletError> {
    let command = ledger_transport::APDUCommand {
        cla: LEDGER_CLA,
        ins: LEDGER_INS_GET_PUBLIC_KEY,
        p1: if display_confirm {
            LEDGER_P1_GET_PUBLIC_KEY_DISPLAY
        } else {
            LEDGER_P1_GET_PUBLIC_KEY_SILENT
        },
        p2: LEDGER_NETWORK_ID,
        data: ledger_hd_path_to_bytes(derivation_path),
    };
    let response = ledger_exchange(&command)?;
    if response.retcode() != LEDGER_RETURN_CODE_OK {
        return Err(map_ledger_retcode(response.retcode()));
    }
    let data = response.data();
    if data.len() != 32 {
        return Err(HardwareWalletError::new(
            "ERR_HARDWARE_TRANSPORT",
            format!(
                "ledger public key response length {} is invalid",
                data.len()
            ),
        ));
    }
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(data);
    Ok(PublicKey::ED25519(near_crypto::ED25519PublicKey::from(
        bytes,
    )))
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
fn ledger_sign_transaction(
    unsigned_tx: &[u8],
    derivation_path: &slipped10::BIP32Path,
) -> Result<near_crypto::Signature, HardwareWalletError> {
    // Use a single transport for the entire signing operation (all chunks
    // must go over the same HID connection).
    let transport = ledger_get_transport()?;

    // Reset Ledger NEAR app state to clear any partially-filled buffer from
    // a previous interrupted operation (mirrors near-ledger-js behavior).
    let version_cmd = ledger_transport::APDUCommand {
        cla: LEDGER_CLA,
        ins: LEDGER_INS_GET_VERSION,
        p1: 0x00,
        p2: 0x00,
        data: vec![],
    };
    match transport.exchange(&version_cmd) {
        Ok(resp) => {
            let data = resp.data();
            if data.len() >= 3 {
                log::info!(
                    "ledger NEAR app version: {}.{}.{}",
                    data[0], data[1], data[2],
                );
            }
        }
        Err(e) => {
            log::warn!("ledger getVersion (state reset) failed: {e:?}");
        }
    }

    let mut data = ledger_hd_path_to_bytes(derivation_path);
    data.extend_from_slice(unsigned_tx);
    let chunks: Vec<&[u8]> = data.chunks(LEDGER_CHUNK_SIZE).collect();
    log::info!(
        "ledger sign: tx_bytes={} total_payload={} chunks={}",
        unsigned_tx.len(),
        data.len(),
        chunks.len(),
    );
    for (idx, chunk) in chunks.iter().enumerate() {
        let is_last = idx + 1 == chunks.len();
        let command = ledger_transport::APDUCommand {
            cla: LEDGER_CLA,
            ins: LEDGER_INS_SIGN_TRANSACTION,
            p1: if is_last {
                LEDGER_P1_SIGN_LAST_CHUNK
            } else {
                LEDGER_P1_SIGN_CHUNK
            },
            p2: LEDGER_NETWORK_ID,
            data: chunk.to_vec(),
        };
        let response = transport
            .exchange(&command)
            .map_err(|e| map_ledger_transport_error(format!("{e:?}")))?;
        if response.retcode() != LEDGER_RETURN_CODE_OK {
            return Err(map_ledger_retcode(response.retcode()));
        }
        if is_last {
            return near_crypto::Signature::from_parts(
                near_crypto::KeyType::ED25519,
                response.data(),
            )
            .map_err(|e| {
                HardwareWalletError::new(
                    "ERR_HARDWARE_TRANSPORT",
                    format!("ledger signature decode failed: {e}"),
                )
            });
        }
    }
    Err(HardwareWalletError::new(
        "ERR_HARDWARE_TRANSPORT",
        "ledger signing produced no response",
    ))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn ledger_open_near_app() -> Result<(), HardwareWalletError> {
    Err(HardwareWalletError::new(
        "ERR_UNAVAILABLE",
        "hardware wallet support is unavailable on this platform",
    ))
}

pub(crate) fn hardware_wallet_supported() -> bool {
    match hardware_wallet_adapter().as_str() {
        "none" => false,
        "mock" => true,
        "auto" | "hid" => cfg!(any(
            target_os = "macos",
            target_os = "linux",
            target_os = "windows"
        )),
        _ => false,
    }
}

#[allow(clippy::needless_return)]
pub(crate) fn hardware_wallet_get_public_key(
    wallet_type: &str,
    derivation_path: &str,
    display_confirm: bool,
) -> Result<PublicKey, HardwareWalletError> {
    if wallet_type != HARDWARE_WALLET_TYPE_LEDGER {
        return Err(HardwareWalletError::new(
            "ERR_PARAMS",
            format!("unsupported wallet_type '{wallet_type}'"),
        ));
    }
    let adapter = hardware_wallet_adapter();
    match adapter.as_str() {
        "none" => Err(HardwareWalletError::new(
            "ERR_UNAVAILABLE",
            "hardware wallet adapter is disabled",
        )),
        "mock" => Ok(mock_ledger_get_public_key(derivation_path)),
        "auto" | "hid" => {
            let path = parse_ledger_derivation_path(Some(derivation_path))?;
            #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
            {
                ledger_open_near_app()?;
                return ledger_get_public_key(&path, display_confirm);
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
            {
                let _ = (path, display_confirm);
                Err(HardwareWalletError::new(
                    "ERR_UNAVAILABLE",
                    "hardware wallet support is unavailable on this platform",
                ))
            }
        }
        _ => Err(HardwareWalletError::new(
            "ERR_UNAVAILABLE",
            format!("unknown hardware wallet adapter '{adapter}'"),
        )),
    }
}

#[allow(clippy::needless_return)]
pub(crate) fn hardware_wallet_sign_transaction(
    wallet_type: &str,
    derivation_path: &str,
    unsigned_tx: &[u8],
) -> Result<near_crypto::Signature, HardwareWalletError> {
    if wallet_type != HARDWARE_WALLET_TYPE_LEDGER {
        return Err(HardwareWalletError::new(
            "ERR_PARAMS",
            format!("unsupported wallet_type '{wallet_type}'"),
        ));
    }
    let adapter = hardware_wallet_adapter();
    match adapter.as_str() {
        "none" => Err(HardwareWalletError::new(
            "ERR_UNAVAILABLE",
            "hardware wallet adapter is disabled",
        )),
        "mock" => mock_ledger_sign_transaction(unsigned_tx, derivation_path),
        "auto" | "hid" => {
            let path = parse_ledger_derivation_path(Some(derivation_path))?;
            #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
            {
                ledger_open_near_app()?;
                return ledger_sign_transaction(unsigned_tx, &path);
            }
            #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
            {
                let _ = (path, unsigned_tx);
                Err(HardwareWalletError::new(
                    "ERR_UNAVAILABLE",
                    "hardware wallet support is unavailable on this platform",
                ))
            }
        }
        _ => Err(HardwareWalletError::new(
            "ERR_UNAVAILABLE",
            format!("unknown hardware wallet adapter '{adapter}'"),
        )),
    }
}

pub(crate) fn resolve_hardware_wallet_record(
    network: &str,
    account_id: &str,
    public_key: &str,
) -> Result<HardwareWalletIndexRecord, HardwareWalletError> {
    let Some(settings) = load_signing_settings().0 else {
        return Err(HardwareWalletError::new(
            "ERR_HARDWARE_KEY_NOT_ON_ACCOUNT",
            format!("hardware wallet key {network}:{account_id}:{public_key} is not connected yet"),
        ));
    };
    hardware_wallet_record_for_key(&settings, network, account_id, public_key).ok_or_else(|| {
        HardwareWalletError::new(
            "ERR_HARDWARE_KEY_NOT_ON_ACCOUNT",
            format!("hardware wallet key {network}:{account_id}:{public_key} is not connected yet"),
        )
    })
}

pub(crate) fn connect_hardware_wallet_result(
    params: &Value,
    settings_lock: &Mutex<()>,
) -> Result<Value, (&'static str, String)> {
    let network =
        crate::broker::resolve_network_param(params, "mainnet").map_err(|e| ("ERR_PARAMS", e))?;
    let credentials_home_dir =
        crate::signing::resolve_credentials_home_dir(params).map_err(|e| ("ERR_IO", e))?;

    let requested_account_id = crate::broker::parse_string(params, "account_id")
        .map(|raw| crate::config::validate_account_id_param(raw))
        .transpose()
        .map_err(|e| ("ERR_PARAMS", e))?;
    let wallet_type = crate::broker::parse_string(params, "wallet_type")
        .unwrap_or(HARDWARE_WALLET_TYPE_LEDGER)
        .to_ascii_lowercase();
    if wallet_type != HARDWARE_WALLET_TYPE_LEDGER {
        return Err((
            "ERR_PARAMS",
            format!("unsupported wallet_type '{wallet_type}'"),
        ));
    }
    let derivation_path = crate::broker::parse_string(params, "derivation_path")
        .unwrap_or(DEFAULT_LEDGER_DERIVATION_PATH)
        .trim()
        .to_string();
    let display_confirm = crate::broker::parse_bool(params, "display_confirm", true);

    let public_key =
        hardware_wallet_get_public_key(&wallet_type, &derivation_path, display_confirm)
            .map_err(|e| (e.code, e.message))?;
    let public_key_str = public_key.to_string();
    let implicit_account_id =
        implicit_account_id_from_public_key(&public_key).map_err(|e| (e.code, e.message))?;
    let account_id = requested_account_id
        .clone()
        .unwrap_or_else(|| implicit_account_id.clone());
    let account_binding = if requested_account_id.is_some() {
        "selected_account"
    } else {
        "implicit_account"
    };

    let permission = if let Some(requested_account_id) = requested_account_id.as_ref() {
        let onchain_keys =
            fetch_onchain_access_keys(requested_account_id).map_err(|e| ("ERR_IO", e))?;
        let Some(onchain_key) = onchain_keys
            .iter()
            .find(|item| item.public_key.trim() == public_key_str)
        else {
            return Err((
                "ERR_HARDWARE_KEY_NOT_ON_ACCOUNT",
                format!(
                    "ledger key {public_key_str} is not an access key on account {requested_account_id}"
                ),
            ));
        };
        access_key_permission_to_summary(&onchain_key.access_key)
    } else {
        match fetch_onchain_access_keys(&account_id) {
            Ok(onchain_keys) => onchain_keys
                .iter()
                .find(|item| item.public_key.trim() == public_key_str)
                .map(|item| access_key_permission_to_summary(&item.access_key))
                .unwrap_or_else(|| json!({ "kind": "unknown" })),
            Err(_) => json!({ "kind": "unknown" }),
        }
    };

    let _guard = settings_lock.lock().unwrap();
    let mut settings = load_signing_settings().0.unwrap_or_else(|| json!({}));
    if !settings.is_object() {
        settings = json!({});
    }

    upsert_hardware_wallet_index(
        &mut settings,
        &[HardwareWalletIndexRecord {
            network: network.clone(),
            account_id: account_id.clone(),
            public_key: public_key_str.clone(),
            wallet_type: wallet_type.clone(),
            derivation_path: derivation_path.clone(),
            last_seen_at_ms: now_ms(),
        }],
    );
    upsert_signing_key_index(
        &mut settings,
        &[IndexedSigningKeyRecord {
            network: network.clone(),
            account_id: account_id.clone(),
            public_key: public_key_str.clone(),
            label: None,
            available_sources: vec![SOURCE_HARDWARE_WALLET.to_string()],
            in_nearxd_keychain: nearxd_keychain_has_scoped_credential(
                &network,
                &account_id,
                &public_key_str,
            ),
            nearxd_keychain_protection: None,
            last_seen_at_ms: now_ms(),
        }],
    );

    let settings_store =
        persist_signing_settings(&settings).map_err(|e| ("ERR_PERSIST", e))?;
    drop(_guard);

    let mut available_sources = BTreeSet::new();
    available_sources.insert(SOURCE_HARDWARE_WALLET.to_string());
    if nearxd_keychain_has_scoped_credential(&network, &account_id, &public_key_str) {
        available_sources.insert(SOURCE_NEARXD_KEYCHAIN.to_string());
    }
    if near_cli_secure_has_credential(&network, &account_id, &public_key_str) {
        available_sources.insert(SOURCE_NEAR_CLI_SECURE.to_string());
    }
    let legacy_dir = credentials_home_dir.join(&network);
    if legacy_dir.exists() {
        if let Ok(legacy) = collect_legacy_credentials(&legacy_dir) {
            if legacy.iter().any(|item| {
                item.credential.account_id == account_id
                    && item.credential.public_key == public_key_str
            }) {
                available_sources.insert(SOURCE_LEGACY_FILE.to_string());
            }
        }
    }
    let preferred_source = preferred_source_from_set(&available_sources);
    let ordered_sources = ordered_sources_from_set(&available_sources);
    let in_nearxd_keychain = available_sources.contains(SOURCE_NEARXD_KEYCHAIN);
    let importable = !in_nearxd_keychain
        && (available_sources.contains(SOURCE_LEGACY_FILE)
            || available_sources.contains(SOURCE_NEAR_CLI_SECURE));
    let label = signing_key_label(&settings, &network, &account_id, &public_key_str);

    Ok(json!({
        "network": network,
        "wallet_type": wallet_type,
        "account_id": account_id,
        "requested_account_id": requested_account_id,
        "implicit_account_id": implicit_account_id,
        "account_binding": account_binding,
        "public_key": public_key_str.clone(),
        "label": label,
        "curve_type": credential_curve_type(&public_key_str),
        "permission": permission,
        "available_sources": ordered_sources,
        "preferred_source": preferred_source,
        "in_nearxd_keychain": in_nearxd_keychain,
        "importable": importable,
        "derivation_path": derivation_path,
        "storage_backend": crate::keychain::secure_store_backend_name(),
        "settings_save": {
            "saved": true,
            "source": settings_store,
        },
    }))
}
