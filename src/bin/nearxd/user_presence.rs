use serde_json::{json, Value};
use std::env;

pub(crate) const DEFAULT_USER_PRESENCE_REASON: &str = "NEARx needs your approval to continue.";

fn user_presence_adapter() -> String {
    env::var("NEARXD_USER_PRESENCE_ADAPTER")
        .unwrap_or_else(|_| "auto".to_string())
        .trim()
        .to_ascii_lowercase()
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

    crate::util::run_swift_json(script, &[if allow_fallback { "1" } else { "0" }])
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

    crate::util::run_swift_json(script, &[reason, if allow_fallback { "1" } else { "0" }])
}

#[allow(unused_variables)] // params used on macOS via cfg blocks
pub(crate) fn probe_user_presence(allow_fallback: bool) -> Value {
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

#[allow(unused_variables)] // params used on macOS via cfg blocks
pub(crate) fn request_user_presence(reason: &str, allow_fallback: bool) -> Result<Value, String> {
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
