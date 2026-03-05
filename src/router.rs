//! Strict deep-link router for NEARx.
//!
//! Canonical URIs use `nearx://v1/...` and are parsed into typed route variants.
//! Compatibility aliases (`near://...`, `nearx://tx/...`, etc.) are accepted on
//! input and normalized to canonical form.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Network {
    Mainnet,
    Testnet,
    Betanet,
    Localnet,
    Custom,
}

impl Network {
    fn parse(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "mainnet" => Some(Self::Mainnet),
            "testnet" => Some(Self::Testnet),
            "betanet" => Some(Self::Betanet),
            "localnet" => Some(Self::Localnet),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    fn as_str(&self) -> &'static str {
        match self {
            Self::Mainnet => "mainnet",
            Self::Testnet => "testnet",
            Self::Betanet => "betanet",
            Self::Localnet => "localnet",
            Self::Custom => "custom",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockRef {
    Height(u64),
    Hash(String),
}

impl BlockRef {
    fn as_string(&self) -> String {
        match self {
            Self::Height(h) => h.to_string(),
            Self::Hash(h) => h.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteV1 {
    Home,
    Tx {
        tx_hash: String,
        block: Option<BlockRef>,
        network: Option<Network>,
    },
    Block {
        block: BlockRef,
        network: Option<Network>,
    },
    Account {
        account_id: String,
        network: Option<Network>,
    },
    Contract {
        account_id: String,
        method: Option<String>,
        network: Option<Network>,
    },
    AccessKey {
        account_id: String,
        public_key: String,
        network: Option<Network>,
    },
    Staking {
        account_id: Option<String>,
        network: Option<Network>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    V1(RouteV1),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedDeepLink {
    pub route: Route,
    pub canonical_uri: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Scheme,
    Version,
    Route,
    SegmentCount,
    QueryKey(String),
    IdentifierFormat(String),
    Decode,
    Empty,
}

impl ParseError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Scheme => "ERR_SCHEME",
            Self::Version => "ERR_VERSION",
            Self::Route => "ERR_ROUTE",
            Self::SegmentCount => "ERR_SEGMENT_COUNT",
            Self::QueryKey(_) => "ERR_QUERY_KEY",
            Self::IdentifierFormat(_) => "ERR_IDENTIFIER_FORMAT",
            Self::Decode => "ERR_DECODE",
            Self::Empty => "ERR_ROUTE",
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scheme => write!(f, "unsupported deep-link scheme"),
            Self::Version => write!(f, "unsupported route version"),
            Self::Route => write!(f, "unsupported route"),
            Self::SegmentCount => write!(f, "invalid route segment count"),
            Self::QueryKey(k) => write!(f, "unsupported query key: {k}"),
            Self::IdentifierFormat(v) => write!(f, "invalid identifier format: {v}"),
            Self::Decode => write!(f, "failed to decode deep-link input"),
            Self::Empty => write!(f, "empty deep-link input"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse from deep-link or web hash route. Returns `None` for invalid inputs.
///
/// Supported forms:
/// - `nearx://v1/...` (canonical)
/// - `near://...` (compatibility input)
/// - `nearx://tx/...` / `nearx://block/...` aliases
/// - `#/v1/...` and `/v1/...`
/// - `#/deeplink/<encodeURIComponent(nearx://...)>`
pub fn parse(raw: &str) -> Option<Route> {
    parse_any(raw).ok()
}

/// Parse and canonicalize a deep-link URI.
///
/// This function is strict and intended for deep-link transport boundaries
/// (broker, native host, desktop entry points).
pub fn parse_deep_link(raw: &str) -> Result<ParsedDeepLink, ParseError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(ParseError::Empty);
    }

    let normalized = normalize_scheme(s)?;
    let (path_part, query_part) = split_path_and_query(&normalized);
    let path_segments = decode_segments(path_part)?;
    let query = parse_query_map(query_part)?;

    let route_v1 = parse_route_v1(&path_segments, &query)?;
    let route = Route::V1(route_v1);
    let canonical_uri = canonical_uri(&route);

    Ok(ParsedDeepLink {
        route,
        canonical_uri,
    })
}

fn parse_any(raw: &str) -> Result<Route, ParseError> {
    let s = raw.trim();
    if s.is_empty() {
        return Ok(Route::V1(RouteV1::Home));
    }

    if let Some(encoded) = s.strip_prefix("#/deeplink/") {
        let decoded = urlencoding::decode(encoded).map_err(|_| ParseError::Decode)?;
        return parse_deep_link(decoded.as_ref()).map(|p| p.route);
    }

    if let Some(path) = s.strip_prefix("#/") {
        return parse_deep_link(&format!("nearx://{path}")).map(|p| p.route);
    }

    if let Some(path) = s.strip_prefix('/') {
        return parse_deep_link(&format!("nearx://{path}")).map(|p| p.route);
    }

    if s.starts_with("v1/") || s.eq_ignore_ascii_case("v1") {
        return parse_deep_link(&format!("nearx://{s}")).map(|p| p.route);
    }

    parse_deep_link(s).map(|p| p.route)
}

fn normalize_scheme(raw: &str) -> Result<String, ParseError> {
    let s = raw.trim();

    let s = if let Some(rest) = s.strip_prefix("web+nearx:") {
        format!("nearx:{rest}")
    } else if let Some(rest) = s.strip_prefix("web+near:") {
        format!("near:{rest}")
    } else {
        s.to_string()
    };

    let Some((scheme, rest)) = s.split_once(':') else {
        return Err(ParseError::Scheme);
    };

    if !scheme.eq_ignore_ascii_case("nearx") && !scheme.eq_ignore_ascii_case("near") {
        return Err(ParseError::Scheme);
    }

    let mut rest = rest;
    while rest.starts_with('/') {
        rest = &rest[1..];
    }

    Ok(format!("nearx://{rest}"))
}

fn split_path_and_query(normalized: &str) -> (&str, Option<&str>) {
    let rest = normalized.strip_prefix("nearx://").unwrap_or(normalized);
    let no_fragment = rest.split('#').next().unwrap_or(rest);
    if let Some((path, query)) = no_fragment.split_once('?') {
        (path, Some(query))
    } else {
        (no_fragment, None)
    }
}

fn decode_segments(path: &str) -> Result<Vec<String>, ParseError> {
    let mut out = Vec::new();
    for seg in path.split('/').filter(|s| !s.is_empty()) {
        let decoded = urlencoding::decode(seg).map_err(|_| ParseError::Decode)?;
        out.push(decoded.into_owned());
    }
    Ok(out)
}

fn parse_query_map(query: Option<&str>) -> Result<BTreeMap<String, String>, ParseError> {
    let mut map = BTreeMap::new();
    let Some(q) = query else {
        return Ok(map);
    };

    for pair in q.split('&').filter(|p| !p.is_empty()) {
        let (k_raw, v_raw) = if let Some((k, v)) = pair.split_once('=') {
            (k, v)
        } else {
            (pair, "")
        };

        let k = urlencoding::decode(k_raw)
            .map_err(|_| ParseError::Decode)?
            .into_owned();
        let v = urlencoding::decode(v_raw)
            .map_err(|_| ParseError::Decode)?
            .into_owned();

        if map.contains_key(&k) {
            return Err(ParseError::QueryKey(k));
        }
        map.insert(k, v);
    }

    Ok(map)
}

fn parse_route_v1(
    segments: &[String],
    query: &BTreeMap<String, String>,
) -> Result<RouteV1, ParseError> {
    if segments.is_empty() {
        return Err(ParseError::Version);
    }

    let first = segments[0].to_ascii_lowercase();
    if first == "v1" {
        parse_v1_canonical(segments, query)
    } else if first.starts_with('v')
        && first.len() > 1
        && first[1..].chars().all(|c| c.is_ascii_digit())
    {
        Err(ParseError::Version)
    } else {
        parse_v1_alias(segments, query)
    }
}

fn parse_v1_canonical(
    segments: &[String],
    query: &BTreeMap<String, String>,
) -> Result<RouteV1, ParseError> {
    let page = segments
        .get(1)
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_else(|| "home".to_string());

    match page.as_str() {
        "home" => {
            ensure_segment_len(segments, &[1, 2])?;
            ensure_query_keys(query, &["network"])?;
            if query.contains_key("network") {
                return Err(ParseError::QueryKey("network".to_string()));
            }
            Ok(RouteV1::Home)
        }
        "tx" => {
            ensure_segment_len(segments, &[3])?;
            ensure_query_keys(query, &["block", "network"])?;

            let tx_hash = segments[2].clone();
            validate_tx_hash(&tx_hash)?;

            let block = query.get("block").map(|v| parse_block_ref(v)).transpose()?;
            let network = parse_network_query(query)?;

            Ok(RouteV1::Tx {
                tx_hash,
                block,
                network,
            })
        }
        "block" => {
            ensure_segment_len(segments, &[3])?;
            ensure_query_keys(query, &["network"])?;

            let block = parse_block_ref(&segments[2])?;
            let network = parse_network_query(query)?;

            Ok(RouteV1::Block { block, network })
        }
        "account" => {
            ensure_segment_len(segments, &[3])?;
            ensure_query_keys(query, &["network"])?;

            let account_id = segments[2].clone();
            validate_account_id(&account_id)?;
            let network = parse_network_query(query)?;

            Ok(RouteV1::Account {
                account_id,
                network,
            })
        }
        "contract" => {
            ensure_segment_len(segments, &[3])?;
            ensure_query_keys(query, &["method", "network"])?;

            let account_id = segments[2].clone();
            validate_account_id(&account_id)?;

            let method = query.get("method").cloned();
            if let Some(m) = method.as_deref() {
                validate_method_name(m)?;
            }

            let network = parse_network_query(query)?;

            Ok(RouteV1::Contract {
                account_id,
                method,
                network,
            })
        }
        "access-key" => {
            ensure_segment_len(segments, &[4])?;
            ensure_query_keys(query, &["network"])?;

            let account_id = segments[2].clone();
            validate_account_id(&account_id)?;

            let public_key = segments[3].clone();
            validate_public_key(&public_key)?;

            let network = parse_network_query(query)?;

            Ok(RouteV1::AccessKey {
                account_id,
                public_key,
                network,
            })
        }
        "staking" => {
            ensure_segment_len(segments, &[2])?;
            ensure_query_keys(query, &["account", "network"])?;

            let account_id = query.get("account").cloned();
            if let Some(ref aid) = account_id {
                validate_account_id(aid)?;
            }
            let network = parse_network_query(query)?;

            Ok(RouteV1::Staking {
                account_id,
                network,
            })
        }
        _ => Err(ParseError::Route),
    }
}

fn parse_v1_alias(
    segments: &[String],
    query: &BTreeMap<String, String>,
) -> Result<RouteV1, ParseError> {
    let page = segments
        .first()
        .map(|s| s.to_ascii_lowercase())
        .ok_or(ParseError::Route)?;

    match page.as_str() {
        "home" => {
            ensure_segment_len(segments, &[1])?;
            ensure_query_keys(query, &[])?;
            Ok(RouteV1::Home)
        }
        "tx" => {
            ensure_segment_len(segments, &[2, 3])?;
            ensure_query_keys(query, &["block", "network"])?;

            let (tx_hash, block) = if segments.len() == 3 {
                let block = parse_block_ref(&segments[1])?;
                (segments[2].clone(), Some(block))
            } else {
                let block = query.get("block").map(|v| parse_block_ref(v)).transpose()?;
                (segments[1].clone(), block)
            };

            validate_tx_hash(&tx_hash)?;
            let network = parse_network_query(query)?;

            Ok(RouteV1::Tx {
                tx_hash,
                block,
                network,
            })
        }
        "block" => {
            ensure_segment_len(segments, &[2])?;
            ensure_query_keys(query, &["network"])?;

            let block = parse_block_ref(&segments[1])?;
            let network = parse_network_query(query)?;

            Ok(RouteV1::Block { block, network })
        }
        "account" => {
            ensure_segment_len(segments, &[2])?;
            ensure_query_keys(query, &["network"])?;

            let account_id = segments[1].clone();
            validate_account_id(&account_id)?;
            let network = parse_network_query(query)?;

            Ok(RouteV1::Account {
                account_id,
                network,
            })
        }
        "contract" => {
            ensure_segment_len(segments, &[2])?;
            ensure_query_keys(query, &["method", "network"])?;

            let account_id = segments[1].clone();
            validate_account_id(&account_id)?;

            let method = query.get("method").cloned();
            if let Some(m) = method.as_deref() {
                validate_method_name(m)?;
            }

            let network = parse_network_query(query)?;

            Ok(RouteV1::Contract {
                account_id,
                method,
                network,
            })
        }
        "access-key" => {
            ensure_segment_len(segments, &[3])?;
            ensure_query_keys(query, &["network"])?;

            let account_id = segments[1].clone();
            validate_account_id(&account_id)?;

            let public_key = segments[2].clone();
            validate_public_key(&public_key)?;

            let network = parse_network_query(query)?;

            Ok(RouteV1::AccessKey {
                account_id,
                public_key,
                network,
            })
        }
        "staking" => {
            ensure_segment_len(segments, &[1])?;
            ensure_query_keys(query, &["account", "network"])?;

            let account_id = query.get("account").cloned();
            if let Some(ref aid) = account_id {
                validate_account_id(aid)?;
            }
            let network = parse_network_query(query)?;

            Ok(RouteV1::Staking {
                account_id,
                network,
            })
        }
        _ => Err(ParseError::Route),
    }
}

fn parse_network_query(query: &BTreeMap<String, String>) -> Result<Option<Network>, ParseError> {
    query
        .get("network")
        .map(|v| {
            Network::parse(v).ok_or_else(|| ParseError::IdentifierFormat(format!("network:{v}")))
        })
        .transpose()
}

fn ensure_segment_len(segments: &[String], accepted: &[usize]) -> Result<(), ParseError> {
    if accepted.contains(&segments.len()) {
        Ok(())
    } else {
        Err(ParseError::SegmentCount)
    }
}

fn ensure_query_keys(query: &BTreeMap<String, String>, allowed: &[&str]) -> Result<(), ParseError> {
    for key in query.keys() {
        if !allowed.iter().any(|k| *k == key) {
            return Err(ParseError::QueryKey(key.clone()));
        }
    }
    Ok(())
}

fn parse_block_ref(raw: &str) -> Result<BlockRef, ParseError> {
    if raw.chars().all(|c| c.is_ascii_digit()) {
        let h = raw
            .parse::<u64>()
            .map_err(|_| ParseError::IdentifierFormat(format!("blockRef:{raw}")))?;
        if h == 0 {
            return Err(ParseError::IdentifierFormat(format!("blockRef:{raw}")));
        }
        return Ok(BlockRef::Height(h));
    }

    if is_base58(raw) && (43..=44).contains(&raw.len()) {
        return Ok(BlockRef::Hash(raw.to_string()));
    }

    Err(ParseError::IdentifierFormat(format!("blockRef:{raw}")))
}

fn validate_tx_hash(tx_hash: &str) -> Result<(), ParseError> {
    if is_base58(tx_hash) && (43..=44).contains(&tx_hash.len()) {
        Ok(())
    } else {
        Err(ParseError::IdentifierFormat(format!("txHash:{tx_hash}")))
    }
}

fn validate_account_id(account_id: &str) -> Result<(), ParseError> {
    if !(2..=64).contains(&account_id.len()) {
        return Err(ParseError::IdentifierFormat(format!(
            "accountId:{account_id}"
        )));
    }

    if account_id.len() == 64 && account_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return Ok(());
    }

    if !account_id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-')
    {
        return Err(ParseError::IdentifierFormat(format!(
            "accountId:{account_id}"
        )));
    }

    Ok(())
}

fn validate_public_key(public_key: &str) -> Result<(), ParseError> {
    let Some((curve, payload)) = public_key.split_once(':') else {
        return Err(ParseError::IdentifierFormat(format!(
            "publicKey:{public_key}"
        )));
    };

    if curve != "ed25519" && curve != "secp256k1" {
        return Err(ParseError::IdentifierFormat(format!(
            "publicKey:{public_key}"
        )));
    }

    if !is_base58(payload) || !(32..=128).contains(&payload.len()) {
        return Err(ParseError::IdentifierFormat(format!(
            "publicKey:{public_key}"
        )));
    }

    Ok(())
}

fn validate_method_name(method: &str) -> Result<(), ParseError> {
    if method.is_empty() || method.len() > 128 {
        return Err(ParseError::IdentifierFormat(format!("method:{method}")));
    }

    if !method
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$' || c == '-')
    {
        return Err(ParseError::IdentifierFormat(format!("method:{method}")));
    }

    Ok(())
}

fn is_base58(s: &str) -> bool {
    const B58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    !s.is_empty() && s.chars().all(|c| B58.contains(c))
}

fn canonical_uri(route: &Route) -> String {
    match route {
        Route::V1(RouteV1::Home) => "nearx://v1/home".to_string(),
        Route::V1(RouteV1::Tx {
            tx_hash,
            block,
            network,
        }) => {
            let mut q = BTreeMap::new();
            if let Some(b) = block {
                q.insert("block".to_string(), b.as_string());
            }
            if let Some(n) = network {
                q.insert("network".to_string(), n.as_str().to_string());
            }
            with_query(format!("nearx://v1/tx/{tx_hash}"), &q)
        }
        Route::V1(RouteV1::Block { block, network }) => {
            let mut q = BTreeMap::new();
            if let Some(n) = network {
                q.insert("network".to_string(), n.as_str().to_string());
            }
            with_query(format!("nearx://v1/block/{}", block.as_string()), &q)
        }
        Route::V1(RouteV1::Account {
            account_id,
            network,
        }) => {
            let mut q = BTreeMap::new();
            if let Some(n) = network {
                q.insert("network".to_string(), n.as_str().to_string());
            }
            with_query(format!("nearx://v1/account/{account_id}"), &q)
        }
        Route::V1(RouteV1::Contract {
            account_id,
            method,
            network,
        }) => {
            let mut q = BTreeMap::new();
            if let Some(m) = method {
                q.insert("method".to_string(), m.clone());
            }
            if let Some(n) = network {
                q.insert("network".to_string(), n.as_str().to_string());
            }
            with_query(format!("nearx://v1/contract/{account_id}"), &q)
        }
        Route::V1(RouteV1::AccessKey {
            account_id,
            public_key,
            network,
        }) => {
            let mut q = BTreeMap::new();
            if let Some(n) = network {
                q.insert("network".to_string(), n.as_str().to_string());
            }
            with_query(
                format!("nearx://v1/access-key/{account_id}/{public_key}"),
                &q,
            )
        }
        Route::V1(RouteV1::Staking {
            account_id,
            network,
        }) => {
            let mut q = BTreeMap::new();
            if let Some(a) = account_id {
                q.insert("account".to_string(), a.clone());
            }
            if let Some(n) = network {
                q.insert("network".to_string(), n.as_str().to_string());
            }
            with_query("nearx://v1/staking".to_string(), &q)
        }
    }
}

fn with_query(base: String, query: &BTreeMap<String, String>) -> String {
    if query.is_empty() {
        return base;
    }

    let parts: Vec<String> = query
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
        .collect();

    format!("{base}?{}", parts.join("&"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_tx_with_query() {
        let parsed = parse_deep_link(
            "nearx://v1/tx/6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b?network=mainnet&block=178923456",
        )
        .unwrap();

        assert_eq!(
            parsed.route,
            Route::V1(RouteV1::Tx {
                tx_hash: "6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b".to_string(),
                block: Some(BlockRef::Height(178_923_456)),
                network: Some(Network::Mainnet),
            })
        );

        // Query keys canonicalized in lexical order.
        assert_eq!(
            parsed.canonical_uri,
            "nearx://v1/tx/6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b?block=178923456&network=mainnet"
        );
    }

    #[test]
    fn parses_alias_tx_block_hash_form() {
        let parsed = parse_deep_link(
            "nearx://tx/9xQeWvG816bUx9EPf6hQYy7x6jQW2hRymP7vQ3P7Bh6P/6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b",
        )
        .unwrap();

        assert_eq!(
            parsed.route,
            Route::V1(RouteV1::Tx {
                tx_hash: "6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b".to_string(),
                block: Some(BlockRef::Hash(
                    "9xQeWvG816bUx9EPf6hQYy7x6jQW2hRymP7vQ3P7Bh6P".to_string(),
                )),
                network: None,
            })
        );

        assert_eq!(
            parsed.canonical_uri,
            "nearx://v1/tx/6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b?block=9xQeWvG816bUx9EPf6hQYy7x6jQW2hRymP7vQ3P7Bh6P"
        );
    }

    #[test]
    fn parses_contract_and_access_key_routes() {
        let contract = parse_deep_link("nearx://v1/contract/app.near?method=ft_transfer")
            .unwrap()
            .route;
        assert_eq!(
            contract,
            Route::V1(RouteV1::Contract {
                account_id: "app.near".to_string(),
                method: Some("ft_transfer".to_string()),
                network: None,
            })
        );

        let access_key = parse_deep_link(
            "nearx://v1/access-key/app.near/ed25519:4NfTivU6N3Wy6u9k8i8w8s8qF2g8Xx1R5zqjYQz6YJ8a",
        )
        .unwrap()
        .route;
        assert_eq!(
            access_key,
            Route::V1(RouteV1::AccessKey {
                account_id: "app.near".to_string(),
                public_key: "ed25519:4NfTivU6N3Wy6u9k8i8w8s8qF2g8Xx1R5zqjYQz6YJ8a".to_string(),
                network: None,
            })
        );
    }

    #[test]
    fn near_scheme_is_compat_input() {
        let parsed = parse_deep_link("near://block/178923456").unwrap();
        assert_eq!(
            parsed.canonical_uri,
            "nearx://v1/block/178923456".to_string()
        );
    }

    #[test]
    fn parse_accepts_hash_deeplink_wrapper() {
        let encoded = urlencoding::encode("nearx://v1/home");
        let route = parse(&format!("#/deeplink/{encoded}")).unwrap();
        assert_eq!(route, Route::V1(RouteV1::Home));
    }

    #[test]
    fn parse_accepts_hash_path_wrapper() {
        let route = parse("#/v1/account/intents.near").unwrap();
        assert_eq!(
            route,
            Route::V1(RouteV1::Account {
                account_id: "intents.near".to_string(),
                network: None,
            })
        );
    }

    #[test]
    fn rejects_unknown_query_keys() {
        let err =
            parse_deep_link("nearx://v1/tx/6QfQfA6vG8f2hK4M4iXkTgXfWwqkYw1pXnXGkA9m8t9b?foo=bar")
                .unwrap_err();
        assert_eq!(err.code(), "ERR_QUERY_KEY");
    }

    #[test]
    fn rejects_invalid_identifiers() {
        let bad_hash = parse_deep_link("nearx://v1/tx/not-a-real-hash").unwrap_err();
        assert_eq!(bad_hash.code(), "ERR_IDENTIFIER_FORMAT");

        let bad_account = parse_deep_link("nearx://v1/account/Alice.near").unwrap_err();
        assert_eq!(bad_account.code(), "ERR_IDENTIFIER_FORMAT");
    }

    #[test]
    fn rejects_wrong_version() {
        let err = parse_deep_link("nearx://v2/home").unwrap_err();
        assert_eq!(err.code(), "ERR_VERSION");
    }

    #[test]
    fn empty_parse_defaults_to_home_for_ui() {
        assert_eq!(parse(""), Some(Route::V1(RouteV1::Home)));
    }

    #[test]
    fn parses_staking_route() {
        let parsed = parse_deep_link("nearx://v1/staking").unwrap();
        assert_eq!(
            parsed.route,
            Route::V1(RouteV1::Staking {
                account_id: None,
                network: None,
            })
        );
        assert_eq!(parsed.canonical_uri, "nearx://v1/staking");
    }

    #[test]
    fn parses_staking_with_account() {
        let parsed =
            parse_deep_link("nearx://v1/staking?account=alice.near").unwrap();
        assert_eq!(
            parsed.route,
            Route::V1(RouteV1::Staking {
                account_id: Some("alice.near".to_string()),
                network: None,
            })
        );
        assert_eq!(
            parsed.canonical_uri,
            "nearx://v1/staking?account=alice.near"
        );
    }

    #[test]
    fn parses_staking_alias() {
        let parsed = parse_deep_link("nearx://staking?account=bob.near").unwrap();
        assert_eq!(
            parsed.route,
            Route::V1(RouteV1::Staking {
                account_id: Some("bob.near".to_string()),
                network: None,
            })
        );
    }
}
