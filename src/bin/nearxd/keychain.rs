use serde_json::Value;

#[cfg(target_os = "macos")]
pub(crate) fn keychain_read_generic(service: &str, account: &str) -> Option<String> {
    security_framework::passwords::get_generic_password(service, account)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

#[cfg(target_os = "macos")]
pub(crate) fn keychain_write_generic(
    service: &str,
    account: &str,
    value: &str,
) -> Result<(), String> {
    security_framework::passwords::set_generic_password(service, account, value.as_bytes())
        .map_err(|e| format!("keychain write failed: {e}"))
}

#[cfg(target_os = "macos")]
pub(crate) fn keychain_delete_generic(service: &str, account: &str) -> Result<(), String> {
    // Treat not-found as success for idempotency.
    let _ = security_framework::passwords::delete_generic_password(service, account);
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn keychain_has_generic(service: &str, account: &str) -> bool {
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

#[cfg(not(target_os = "macos"))]
pub(crate) fn keychain_read_generic(_service: &str, _account: &str) -> Option<String> {
    None
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn keychain_write_generic(
    _service: &str,
    _account: &str,
    _value: &str,
) -> Result<(), String> {
    Err("keychain backend unavailable on this platform".to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn keychain_delete_generic(_service: &str, _account: &str) -> Result<(), String> {
    Err("keychain backend unavailable on this platform".to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn keychain_has_generic(_service: &str, _account: &str) -> bool {
    false
}

#[cfg(target_os = "macos")]
pub(crate) fn keychain_write_generic_protected(
    service: &str,
    account: &str,
    value: &str,
    biometry_only: bool,
) -> Result<(), String> {
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

    // SecAccessControlCreateFlags (CFOptionFlags = usize on Apple platforms)
    const USER_PRESENCE: usize = 1 << 0;
    const BIOMETRY_CURRENT_SET: usize = 1 << 3;

    let service_cf = CFString::new(service);
    let account_cf = CFString::new(account);
    let data = CFData::from_buffer(value.as_bytes());

    unsafe {
        // Delete existing item first
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

        // Try protected write (requires code-signed binary with entitlements)
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
                return Ok(());
            }

            // -34018 = errSecMissingEntitlement — binary not code-signed for protected keychain
            if status != -34018 {
                return Err(format!("SecItemAdd failed with status {status}"));
            }
            log::warn!(
                "Protected SecItemAdd failed (-34018), falling back to unprotected keychain"
            );
        }

        // Fallback: store without biometry protection (works for unsigned CLI binaries)
        log::info!("falling back to unprotected keychain write for {service}/{account}");
        security_framework::passwords::set_generic_password(service, account, value.as_bytes())
            .map_err(|e| format!("keychain write fallback failed: {e}"))
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn keychain_read_generic_with_prompt(
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

#[cfg(not(target_os = "macos"))]
pub(crate) fn keychain_write_generic_protected(
    _service: &str,
    _account: &str,
    _value: &str,
    _biometry_only: bool,
) -> Result<(), String> {
    Err("protected keychain writes are unavailable on this platform".to_string())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn keychain_read_generic_with_prompt(
    _service: &str,
    _account: &str,
    _reason: &str,
) -> Result<String, String> {
    Err("keychain prompt reads are unavailable on this platform".to_string())
}

#[cfg(target_os = "macos")]
pub(crate) fn keychain_list_generic_accounts(service: &str) -> Result<Vec<String>, String> {
    let script = r#"
import Foundation
import Security

let service = CommandLine.arguments[1]
let query: [String: Any] = [
    kSecClass as String: kSecClassGenericPassword,
    kSecAttrService as String: service,
    kSecMatchLimit as String: kSecMatchLimitAll,
    kSecReturnAttributes as String: true
]

var item: CFTypeRef?
let status = SecItemCopyMatching(query as CFDictionary, &item)
if status == errSecItemNotFound {
    let out: [String: Any] = ["accounts": []]
    let data = try JSONSerialization.data(withJSONObject: out, options: [])
    print(String(data: data, encoding: .utf8)!)
    exit(0)
}

if status != errSecSuccess {
    fputs("SecItemCopyMatching failed with status \(status)", stderr)
    exit(2)
}

var accounts: [String] = []
if let rows = item as? [[String: Any]] {
    for row in rows {
        if let account = row[kSecAttrAccount as String] as? String {
            accounts.append(account)
        }
    }
} else if let row = item as? [String: Any] {
    if let account = row[kSecAttrAccount as String] as? String {
        accounts.append(account)
    }
}

let out: [String: Any] = ["accounts": accounts]
let data = try JSONSerialization.data(withJSONObject: out, options: [])
print(String(data: data, encoding: .utf8)!)
"#;

    let v = crate::util::run_swift_json(script, &[service])?;
    Ok(v.get("accounts")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn keychain_list_generic_accounts(_service: &str) -> Result<Vec<String>, String> {
    Ok(Vec::new())
}
