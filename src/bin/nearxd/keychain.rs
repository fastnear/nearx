use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtectedWriteOutcome {
    AppliedRequestedProtection,
    FellBackUnprotected,
}

pub(crate) trait SecureStoreBackend: Send + Sync {
    fn backend_name(&self) -> &'static str;
    fn read_generic(&self, service: &str, account: &str) -> Option<String>;
    fn write_generic(&self, service: &str, account: &str, value: &str) -> Result<(), String>;
    fn delete_generic(&self, service: &str, account: &str) -> Result<(), String>;
    fn has_generic(&self, service: &str, account: &str) -> bool;
    fn write_generic_protected(
        &self,
        service: &str,
        account: &str,
        value: &str,
        biometry_only: bool,
    ) -> Result<ProtectedWriteOutcome, String>;
    fn read_generic_with_prompt(
        &self,
        service: &str,
        account: &str,
        reason: &str,
    ) -> Result<String, String>;
    fn list_generic_accounts(&self, service: &str) -> Result<Vec<String>, String>;
}

#[cfg(target_os = "macos")]
struct MacOsSecureStoreBackend;
#[cfg(target_os = "linux")]
struct LinuxSecretServiceBackend;
#[cfg(target_os = "windows")]
struct WindowsCredentialBackend;
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
struct UnsupportedSecureStoreBackend;

#[cfg(target_os = "macos")]
fn backend() -> &'static dyn SecureStoreBackend {
    static BACKEND: MacOsSecureStoreBackend = MacOsSecureStoreBackend;
    &BACKEND
}

#[cfg(target_os = "linux")]
fn backend() -> &'static dyn SecureStoreBackend {
    static BACKEND: LinuxSecretServiceBackend = LinuxSecretServiceBackend;
    &BACKEND
}

#[cfg(target_os = "windows")]
fn backend() -> &'static dyn SecureStoreBackend {
    static BACKEND: WindowsCredentialBackend = WindowsCredentialBackend;
    &BACKEND
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn backend() -> &'static dyn SecureStoreBackend {
    static BACKEND: UnsupportedSecureStoreBackend = UnsupportedSecureStoreBackend;
    &BACKEND
}

pub(crate) fn secure_store_backend_name() -> &'static str {
    backend().backend_name()
}

pub(crate) fn secure_store_prompt_reads_supported() -> bool {
    cfg!(target_os = "macos")
}

pub(crate) fn secure_store_persistence_supported() -> bool {
    !matches!(secure_store_backend_name(), "unsupported")
}

pub(crate) fn keychain_read_generic(service: &str, account: &str) -> Option<String> {
    backend().read_generic(service, account)
}

pub(crate) fn keychain_write_generic(
    service: &str,
    account: &str,
    value: &str,
) -> Result<(), String> {
    backend().write_generic(service, account, value)
}

pub(crate) fn keychain_delete_generic(service: &str, account: &str) -> Result<(), String> {
    backend().delete_generic(service, account)
}

pub(crate) fn keychain_has_generic(service: &str, account: &str) -> bool {
    backend().has_generic(service, account)
}

pub(crate) fn keychain_write_generic_protected(
    service: &str,
    account: &str,
    value: &str,
    biometry_only: bool,
) -> Result<ProtectedWriteOutcome, String> {
    backend().write_generic_protected(service, account, value, biometry_only)
}

pub(crate) fn keychain_read_generic_with_prompt(
    service: &str,
    account: &str,
    reason: &str,
) -> Result<String, String> {
    backend().read_generic_with_prompt(service, account, reason)
}

pub(crate) fn keychain_list_generic_accounts(service: &str) -> Result<Vec<String>, String> {
    backend().list_generic_accounts(service)
}

#[cfg(target_os = "macos")]
impl SecureStoreBackend for MacOsSecureStoreBackend {
    fn backend_name(&self) -> &'static str {
        "macos_keychain"
    }

    fn read_generic(&self, service: &str, account: &str) -> Option<String> {
        macos_keychain_read_generic(service, account)
    }

    fn write_generic(&self, service: &str, account: &str, value: &str) -> Result<(), String> {
        macos_keychain_write_generic(service, account, value)
    }

    fn delete_generic(&self, service: &str, account: &str) -> Result<(), String> {
        macos_keychain_delete_generic(service, account)
    }

    fn has_generic(&self, service: &str, account: &str) -> bool {
        macos_keychain_has_generic(service, account)
    }

    fn write_generic_protected(
        &self,
        service: &str,
        account: &str,
        value: &str,
        biometry_only: bool,
    ) -> Result<ProtectedWriteOutcome, String> {
        macos_keychain_write_generic_protected(service, account, value, biometry_only)
    }

    fn read_generic_with_prompt(
        &self,
        service: &str,
        account: &str,
        reason: &str,
    ) -> Result<String, String> {
        macos_keychain_read_generic_with_prompt(service, account, reason)
    }

    fn list_generic_accounts(&self, service: &str) -> Result<Vec<String>, String> {
        macos_keychain_list_generic_accounts(service)
    }
}

#[cfg(target_os = "linux")]
impl SecureStoreBackend for LinuxSecretServiceBackend {
    fn backend_name(&self) -> &'static str {
        "linux_secret_service"
    }

    fn read_generic(&self, service: &str, account: &str) -> Option<String> {
        generic_store_read_generic(service, account)
    }

    fn write_generic(&self, service: &str, account: &str, value: &str) -> Result<(), String> {
        generic_store_write_generic(service, account, value)
    }

    fn delete_generic(&self, service: &str, account: &str) -> Result<(), String> {
        generic_store_delete_generic(service, account)
    }

    fn has_generic(&self, service: &str, account: &str) -> bool {
        generic_store_has_generic(service, account)
    }

    fn write_generic_protected(
        &self,
        service: &str,
        account: &str,
        value: &str,
        _biometry_only: bool,
    ) -> Result<ProtectedWriteOutcome, String> {
        generic_store_write_generic(service, account, value)?;
        Ok(ProtectedWriteOutcome::AppliedRequestedProtection)
    }

    fn read_generic_with_prompt(
        &self,
        service: &str,
        account: &str,
        _reason: &str,
    ) -> Result<String, String> {
        generic_store_read_generic(service, account)
            .ok_or_else(|| "secure-store item not found".to_string())
    }

    fn list_generic_accounts(&self, service: &str) -> Result<Vec<String>, String> {
        generic_store_list_generic_accounts(service)
    }
}

#[cfg(target_os = "windows")]
impl SecureStoreBackend for WindowsCredentialBackend {
    fn backend_name(&self) -> &'static str {
        "windows_credential_manager"
    }

    fn read_generic(&self, service: &str, account: &str) -> Option<String> {
        generic_store_read_generic(service, account)
    }

    fn write_generic(&self, service: &str, account: &str, value: &str) -> Result<(), String> {
        generic_store_write_generic(service, account, value)
    }

    fn delete_generic(&self, service: &str, account: &str) -> Result<(), String> {
        generic_store_delete_generic(service, account)
    }

    fn has_generic(&self, service: &str, account: &str) -> bool {
        generic_store_has_generic(service, account)
    }

    fn write_generic_protected(
        &self,
        service: &str,
        account: &str,
        value: &str,
        _biometry_only: bool,
    ) -> Result<ProtectedWriteOutcome, String> {
        generic_store_write_generic(service, account, value)?;
        Ok(ProtectedWriteOutcome::AppliedRequestedProtection)
    }

    fn read_generic_with_prompt(
        &self,
        service: &str,
        account: &str,
        _reason: &str,
    ) -> Result<String, String> {
        generic_store_read_generic(service, account)
            .ok_or_else(|| "secure-store item not found".to_string())
    }

    fn list_generic_accounts(&self, service: &str) -> Result<Vec<String>, String> {
        generic_store_list_generic_accounts(service)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
impl SecureStoreBackend for UnsupportedSecureStoreBackend {
    fn backend_name(&self) -> &'static str {
        "unsupported"
    }

    fn read_generic(&self, _service: &str, _account: &str) -> Option<String> {
        None
    }

    fn write_generic(&self, _service: &str, _account: &str, _value: &str) -> Result<(), String> {
        Err("secure-store backend unavailable on this platform".to_string())
    }

    fn delete_generic(&self, _service: &str, _account: &str) -> Result<(), String> {
        Err("secure-store backend unavailable on this platform".to_string())
    }

    fn has_generic(&self, _service: &str, _account: &str) -> bool {
        false
    }

    fn write_generic_protected(
        &self,
        _service: &str,
        _account: &str,
        _value: &str,
        _biometry_only: bool,
    ) -> Result<ProtectedWriteOutcome, String> {
        Err("protected secure-store writes are unavailable on this platform".to_string())
    }

    fn read_generic_with_prompt(
        &self,
        _service: &str,
        _account: &str,
        _reason: &str,
    ) -> Result<String, String> {
        Err("secure-store prompt reads are unavailable on this platform".to_string())
    }

    fn list_generic_accounts(&self, _service: &str) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
}

#[cfg(not(target_os = "macos"))]
fn keyring_entry(service: &str, account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(service, account)
        .map_err(|e| format!("secure-store entry init failed: {e}"))
}

#[cfg(not(target_os = "macos"))]
fn generic_store_read_generic(service: &str, account: &str) -> Option<String> {
    let entry = match keyring_entry(service, account) {
        Ok(entry) => entry,
        Err(err) => {
            log::warn!("nearxd: secure-store init failed for {service}/{account}: {err}");
            return None;
        }
    };
    match entry.get_password() {
        Ok(value) => Some(value),
        Err(keyring::Error::NoEntry) => None,
        Err(err) => {
            log::warn!("nearxd: secure-store read failed for {service}/{account}: {err}");
            None
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn generic_store_write_generic(service: &str, account: &str, value: &str) -> Result<(), String> {
    keyring_entry(service, account)?
        .set_password(value)
        .map_err(|e| format!("secure-store write failed: {e}"))
}

#[cfg(not(target_os = "macos"))]
fn generic_store_delete_generic(service: &str, account: &str) -> Result<(), String> {
    match keyring_entry(service, account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("secure-store delete failed: {e}")),
    }
}

#[cfg(not(target_os = "macos"))]
fn generic_store_has_generic(service: &str, account: &str) -> bool {
    let entry = match keyring_entry(service, account) {
        Ok(entry) => entry,
        Err(_) => return false,
    };
    match entry.get_password() {
        Ok(_) => true,
        Err(keyring::Error::NoEntry) => false,
        Err(err) => {
            log::warn!(
                "nearxd: secure-store existence check failed for {service}/{account}: {err}"
            );
            false
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn generic_store_list_generic_accounts(service: &str) -> Result<Vec<String>, String> {
    let search = keyring_search::Search::new()
        .map_err(|e| format!("secure-store search initialization failed: {e}"))?;
    let results = match search.by_service(service) {
        Ok(results) => results,
        Err(keyring_search::Error::NoResults) => return Ok(Vec::new()),
        Err(e) => return Err(format!("secure-store search failed: {e}")),
    };

    let mut accounts = Vec::new();
    for metadata in results.into_values() {
        if let Some(account) = metadata
            .get("User")
            .or_else(|| metadata.get("username"))
            .or_else(|| metadata.get("account"))
            .or_else(|| metadata.get("acct"))
        {
            accounts.push(account.clone());
        }
    }
    accounts.sort();
    accounts.dedup();
    Ok(accounts)
}

#[cfg(target_os = "macos")]
fn macos_keychain_read_generic(service: &str, account: &str) -> Option<String> {
    security_framework::passwords::get_generic_password(service, account)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

#[cfg(target_os = "macos")]
fn macos_keychain_write_generic(service: &str, account: &str, value: &str) -> Result<(), String> {
    security_framework::passwords::set_generic_password(service, account, value.as_bytes())
        .map_err(|e| format!("keychain write failed: {e}"))
}

#[cfg(target_os = "macos")]
fn macos_keychain_delete_generic(service: &str, account: &str) -> Result<(), String> {
    let _ = security_framework::passwords::delete_generic_password(service, account);
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_keychain_has_generic(service: &str, account: &str) -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::string::CFString;
    use core_foundation_sys::dictionary::{
        kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreateMutable,
        CFDictionarySetValue,
    };
    use security_framework_sys::item::*;
    use security_framework_sys::keychain_item::SecItemCopyMatching;

    let service_cf = CFString::new(service);
    let account_cf = CFString::new(account);

    unsafe {
        let query = CFDictionaryCreateMutable(
            std::ptr::null(),
            0,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        CFDictionarySetValue(query, kSecClass as _, kSecClassGenericPassword as _);
        CFDictionarySetValue(
            query,
            kSecAttrService as _,
            service_cf.as_concrete_TypeRef() as _,
        );
        CFDictionarySetValue(
            query,
            kSecAttrAccount as _,
            account_cf.as_concrete_TypeRef() as _,
        );
        CFDictionarySetValue(
            query,
            kSecReturnAttributes as _,
            CFBoolean::true_value().as_CFTypeRef() as _,
        );

        let status = SecItemCopyMatching(query as _, std::ptr::null_mut());
        core_foundation_sys::base::CFRelease(query as _);
        status == 0
    }
}

#[cfg(target_os = "macos")]
fn macos_keychain_write_generic_protected(
    service: &str,
    account: &str,
    value: &str,
    biometry_only: bool,
) -> Result<ProtectedWriteOutcome, String> {
    use core_foundation::base::TCFType;
    use core_foundation::data::CFData;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::CFRelease;
    use core_foundation_sys::dictionary::{
        kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreateMutable,
        CFDictionarySetValue,
    };
    use security_framework_sys::access_control::{
        kSecAttrAccessibleWhenUnlockedThisDeviceOnly, SecAccessControlCreateWithFlags,
    };
    use security_framework_sys::item::*;
    use security_framework_sys::keychain_item::{SecItemAdd, SecItemDelete};

    const USER_PRESENCE: usize = 1 << 0;
    const BIOMETRY_CURRENT_SET: usize = 1 << 3;

    let service_cf = CFString::new(service);
    let account_cf = CFString::new(account);
    let data = CFData::from_buffer(value.as_bytes());

    unsafe {
        let dq = CFDictionaryCreateMutable(
            std::ptr::null(),
            0,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        CFDictionarySetValue(dq, kSecClass as _, kSecClassGenericPassword as _);
        CFDictionarySetValue(
            dq,
            kSecAttrService as _,
            service_cf.as_concrete_TypeRef() as _,
        );
        CFDictionarySetValue(
            dq,
            kSecAttrAccount as _,
            account_cf.as_concrete_TypeRef() as _,
        );
        SecItemDelete(dq as _);
        CFRelease(dq as _);

        let flags = if biometry_only {
            BIOMETRY_CURRENT_SET
        } else {
            USER_PRESENCE
        };

        let mut error: core_foundation_sys::error::CFErrorRef = std::ptr::null_mut();
        let access = SecAccessControlCreateWithFlags(
            std::ptr::null(),
            kSecAttrAccessibleWhenUnlockedThisDeviceOnly as _,
            flags,
            &mut error,
        );

        if !access.is_null() {
            let aq = CFDictionaryCreateMutable(
                std::ptr::null(),
                0,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            );
            CFDictionarySetValue(aq, kSecClass as _, kSecClassGenericPassword as _);
            CFDictionarySetValue(
                aq,
                kSecAttrService as _,
                service_cf.as_concrete_TypeRef() as _,
            );
            CFDictionarySetValue(
                aq,
                kSecAttrAccount as _,
                account_cf.as_concrete_TypeRef() as _,
            );
            CFDictionarySetValue(aq, kSecValueData as _, data.as_CFTypeRef() as _);
            CFDictionarySetValue(aq, kSecAttrAccessControl as _, access as _);

            let status = SecItemAdd(aq as _, std::ptr::null_mut());
            CFRelease(aq as _);
            CFRelease(access as _);

            if status == 0 {
                return Ok(ProtectedWriteOutcome::AppliedRequestedProtection);
            }

            if status != -34018 {
                return Err(format!("SecItemAdd failed with status {status}"));
            }
            log::warn!(
                "Protected SecItemAdd failed (-34018), falling back to unprotected keychain"
            );
        }

        log::info!("falling back to unprotected keychain write for {service}/{account}");
        security_framework::passwords::set_generic_password(service, account, value.as_bytes())
            .map_err(|e| format!("keychain write fallback failed: {e}"))?;
        Ok(ProtectedWriteOutcome::FellBackUnprotected)
    }
}

#[cfg(target_os = "macos")]
fn macos_keychain_read_generic_with_prompt(
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

    let v = crate::util::run_swift_json(script, &[service, account, reason])?;
    v.get("value")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| "missing value field in keychain read response".to_string())
}

#[cfg(target_os = "macos")]
fn macos_keychain_list_generic_accounts(service: &str) -> Result<Vec<String>, String> {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::string::CFString;
    use core_foundation_sys::array::{
        CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFArrayRef,
    };
    use core_foundation_sys::base::{CFGetTypeID, CFRelease, CFTypeRef};
    use core_foundation_sys::dictionary::{
        kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionaryCreateMutable,
        CFDictionaryGetTypeID, CFDictionaryGetValue, CFDictionaryRef, CFDictionarySetValue,
    };
    use core_foundation_sys::string::CFStringRef;
    use security_framework_sys::base::{errSecItemNotFound, errSecSuccess};
    use security_framework_sys::item::*;
    use security_framework_sys::keychain_item::SecItemCopyMatching;

    unsafe fn account_from_row(row: CFDictionaryRef) -> Option<String> {
        if row.is_null() {
            return None;
        }
        let value = CFDictionaryGetValue(row, kSecAttrAccount as *const _ as *const _);
        if value.is_null() {
            return None;
        }
        let account = CFString::wrap_under_get_rule(value as CFStringRef);
        Some(account.to_string())
    }

    let service_cf = CFString::new(service);

    unsafe {
        let query = CFDictionaryCreateMutable(
            std::ptr::null(),
            0,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        );
        CFDictionarySetValue(query, kSecClass as _, kSecClassGenericPassword as _);
        CFDictionarySetValue(
            query,
            kSecAttrService as _,
            service_cf.as_concrete_TypeRef() as _,
        );
        CFDictionarySetValue(query, kSecMatchLimit as _, kSecMatchLimitAll as _);
        CFDictionarySetValue(
            query,
            kSecReturnAttributes as _,
            CFBoolean::true_value().as_CFTypeRef() as _,
        );

        let mut item: CFTypeRef = std::ptr::null_mut();
        let status = SecItemCopyMatching(query as _, &mut item as *mut _ as *mut _);
        CFRelease(query as _);

        if status == errSecItemNotFound {
            return Ok(Vec::new());
        }
        if status != errSecSuccess {
            return Err(format!("SecItemCopyMatching failed with status {status}"));
        }
        if item.is_null() {
            return Ok(Vec::new());
        }

        let mut accounts = Vec::new();
        let item_type = CFGetTypeID(item);
        if item_type == CFArrayGetTypeID() {
            let rows = item as CFArrayRef;
            let count = CFArrayGetCount(rows);
            for i in 0..count {
                let row = CFArrayGetValueAtIndex(rows, i) as CFDictionaryRef;
                if let Some(account) = account_from_row(row) {
                    accounts.push(account);
                }
            }
        } else if item_type == CFDictionaryGetTypeID() {
            let row = item as CFDictionaryRef;
            if let Some(account) = account_from_row(row) {
                accounts.push(account);
            }
        }
        CFRelease(item as _);

        accounts.sort();
        accounts.dedup();
        Ok(accounts)
    }
}
