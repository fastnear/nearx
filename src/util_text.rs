#[cfg(feature = "near-gas")]
use near_gas::NearGas;
#[cfg(feature = "near-token")]
use near_token::NearToken;

/// Format gas amount in human-readable format
/// Examples: "30 TGas", "5 GGas", "100 Gas"
pub fn format_gas(gas: u64) -> String {
    #[cfg(feature = "near-gas")]
    {
        let near_gas = NearGas::from_gas(gas);
        format!("{}", near_gas)
    }
    #[cfg(not(feature = "near-gas"))]
    {
        // Simple formatter for web build
        const TERA: u64 = 1_000_000_000_000;
        const GIGA: u64 = 1_000_000_000;
        if gas >= TERA {
            format!("{} TGas", gas / TERA)
        } else if gas >= GIGA {
            format!("{} GGas", gas / GIGA)
        } else {
            format!("{gas} Gas")
        }
    }
}

/// Format NEAR token amount in human-readable format
/// Examples: "1 NEAR", "0.5 NEAR", "1000000 yoctoNEAR"
pub fn format_near(yoctonear: u128) -> String {
    #[cfg(feature = "near-token")]
    {
        let token = NearToken::from_yoctonear(yoctonear);
        format!("{}", token)
    }
    #[cfg(not(feature = "near-token"))]
    {
        // Simple formatter for web build
        const NEAR: u128 = 1_000_000_000_000_000_000_000_000;
        if yoctonear == 0 {
            "0 NEAR".to_string()
        } else if yoctonear >= NEAR {
            let near_amount = yoctonear as f64 / NEAR as f64;
            format!("{near_amount:.4} NEAR")
        } else {
            format!("{yoctonear} yoctoNEAR")
        }
    }
}

/// Format gas with compact suffix for UI (e.g., "30T" instead of "30 TGas")
#[allow(dead_code)]
pub fn format_gas_compact(gas: u64) -> String {
    if gas == 0 {
        return "0".to_string();
    }
    const TERA: u64 = 1_000_000_000_000;
    const GIGA: u64 = 1_000_000_000;
    const MEGA: u64 = 1_000_000;

    if gas >= TERA {
        format!("{}T", gas / TERA)
    } else if gas >= GIGA {
        format!("{}G", gas / GIGA)
    } else if gas >= MEGA {
        format!("{}M", gas / MEGA)
    } else {
        gas.to_string()
    }
}

/// Format NEAR amount with compact suffix for UI (e.g., "1.5Ⓝ")
#[allow(dead_code)]
pub fn format_near_compact(yoctonear: u128) -> String {
    if yoctonear == 0 {
        return "0Ⓝ".to_string();
    }
    const NEAR: u128 = 1_000_000_000_000_000_000_000_000;

    if yoctonear >= NEAR {
        let near_amount = yoctonear / NEAR;
        let remainder = yoctonear % NEAR;
        if remainder == 0 {
            format!("{near_amount}Ⓝ")
        } else {
            // Show 1 decimal place
            let decimal = (remainder * 10) / NEAR;
            format!("{near_amount}.{decimal}Ⓝ")
        }
    } else {
        // Sub-NEAR amounts
        format!("{yoctonear}y")
    }
}

// Serialization helpers for WASM targets to handle large numbers
#[cfg(target_arch = "wasm32")]
use serde::Serializer;

/// Serialize u64 as string for JavaScript compatibility
#[cfg(target_arch = "wasm32")]
pub fn serialize_u64_as_string<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}

/// Serialize Option<u64> as string for JavaScript compatibility
#[cfg(target_arch = "wasm32")]
pub fn serialize_option_u64_as_string<S>(
    value: &Option<u64>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match value {
        Some(v) => serializer.serialize_str(&v.to_string()),
        None => serializer.serialize_none(),
    }
}

/// Serialize u128 as string for JavaScript compatibility
#[cfg(target_arch = "wasm32")]
pub fn serialize_u128_as_string<S>(value: &u128, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}
