use anyhow::{anyhow, Result};
use clap::Parser;
use std::env;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Ws,
    Rpc,
}

impl std::str::FromStr for Source {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "ws" | "websocket" => Ok(Source::Ws),
            "rpc" => Ok(Source::Rpc),
            _ => Err(anyhow!("Invalid source '{s}'. Valid options: ws, rpc")),
        }
    }
}

impl std::fmt::Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Ws => write!(f, "ws"),
            Source::Rpc => write!(f, "rpc"),
        }
    }
}

/// NEARx - NEAR Blockchain Transaction Viewer
///
/// High-performance terminal UI for monitoring NEAR Protocol transactions in real-time.
/// Configuration priority: CLI args > Environment variables > Defaults
#[derive(Parser, Debug)]
#[command(name = "nearx")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "NEAR Blockchain Transaction Viewer", long_about = None)]
pub struct CliArgs {
    /// Data source: ws (WebSocket) or rpc (NEAR RPC)
    #[arg(short, long, env = "SOURCE", value_parser = clap::value_parser!(Source))]
    pub source: Option<Source>,

    /// WebSocket server URL
    #[arg(long, env = "WS_URL")]
    pub ws_url: Option<String>,

    /// Fetch full block data via WebSocket
    #[arg(long, env = "WS_FETCH_BLOCKS")]
    pub ws_fetch_blocks: Option<bool>,

    /// NEAR RPC endpoint URL
    #[arg(long, env = "NEAR_NODE_URL")]
    pub near_node_url: Option<String>,

    /// FastNEAR API key (recommended to avoid rate limits)
    /// Env priority: FASTNEAR_API_KEY, then FASTNEAR_AUTH_TOKEN (legacy)
    #[arg(
        long = "fastnear-auth-token",
        visible_alias = "fastnear-api-key",
        env = "FASTNEAR_AUTH_TOKEN"
    )]
    pub fastnear_auth_token: Option<String>,

    /// Archival RPC endpoint URL for fetching historical blocks
    #[arg(long, env = "ARCHIVAL_RPC_URL")]
    pub archival_rpc_url: Option<String>,

    /// FastNEAR Explorer API endpoint URL for fetching full transaction details
    #[arg(long, env = "FASTNEAR_API_URL")]
    pub fastnear_api_url: Option<String>,

    /// RPC polling interval in milliseconds (100-10000)
    #[arg(long, env = "POLL_INTERVAL_MS")]
    pub poll_interval_ms: Option<u64>,

    /// Maximum blocks to catch up per poll (1-100)
    #[arg(long, env = "POLL_MAX_CATCHUP")]
    pub poll_max_catchup: Option<u64>,

    /// Concurrent chunk fetch requests (1-16)
    #[arg(long, env = "POLL_CHUNK_CONCURRENCY")]
    pub poll_chunk_concurrency: Option<usize>,

    /// RPC request timeout in milliseconds (1000-60000)
    #[arg(long, env = "RPC_TIMEOUT_MS")]
    pub rpc_timeout_ms: Option<u64>,

    /// Number of retry attempts for failed RPC requests (0-10)
    #[arg(long, env = "RPC_RETRIES")]
    pub rpc_retries: Option<u32>,

    /// Target UI rendering FPS (1-120)
    #[arg(long, env = "RENDER_FPS")]
    pub render_fps: Option<u32>,

    /// Available FPS options for Ctrl+O cycling (comma-separated, e.g., "20,30,60")
    #[arg(long, env = "RENDER_FPS_CHOICES")]
    pub render_fps_choices: Option<String>,

    /// Number of recent blocks to keep in memory (10-10000)
    #[arg(long, env = "KEEP_BLOCKS")]
    pub keep_blocks: Option<usize>,

    /// Path to SQLite database for persistence
    #[arg(long, env = "SQLITE_DB_PATH")]
    pub sqlite_db_path: Option<String>,

    /// Default filter query to apply on startup (e.g., "acct:intents.near")
    #[arg(long, env = "DEFAULT_FILTER")]
    pub default_filter: Option<String>,

    /// Comma-separated list of NEAR accounts to watch (e.g., "intents.near,alice.near")
    /// Takes precedence over DEFAULT_FILTER
    #[arg(long, env = "WATCH_ACCOUNTS")]
    pub watch_accounts: Option<String>,

    /// Color theme: nord, dos-blue, amber-crt, green-phosphor
    #[arg(long, env = "THEME")]
    pub theme: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub source: Source,
    pub ws_url: String,
    pub ws_fetch_blocks: bool,
    pub render_fps: u32,
    pub render_fps_choices: Vec<u32>,
    pub poll_interval_ms: u64,
    pub poll_max_catchup: u64,
    pub poll_chunk_concurrency: usize,
    pub keep_blocks: usize,
    pub near_node_url: String,
    pub near_node_url_explicit: bool, // true if set via env var or CLI
    pub archival_rpc_url: Option<String>,
    pub fastnear_api_url: String,
    pub rpc_timeout_ms: u64,
    #[allow(dead_code)]
    pub rpc_retries: u32,
    pub fastnear_auth_token: Option<String>,
    pub default_filter: String,
    pub theme: crate::theme::Theme,
}

/// Validate that a value is within a given range (inclusive)
fn validate_in_range<T>(val: T, min: T, max: T, name: &str) -> Result<T>
where
    T: PartialOrd + std::fmt::Display + Copy,
{
    if val < min || val > max {
        Err(anyhow!("{name} must be in range [{min}, {max}], got {val}"))
    } else {
        Ok(val)
    }
}

/// Parse comma-separated FPS list and validate each value
fn parse_fps_list(s: &str) -> Vec<u32> {
    s.split(',')
        .filter_map(|v| v.trim().parse::<u32>().ok())
        .filter(|n| (1..=120).contains(n))
        .collect()
}

/// Load configuration from CLI args and environment variables
/// Priority: CLI args > Environment variables > Defaults
pub fn load() -> Result<Config> {
    let args = CliArgs::parse();

    // Source (with fallback to env var DEFAULT)
    let source = args.source.unwrap_or_else(|| {
        env::var("SOURCE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(Source::Rpc) // Default to RPC for simplicity
    });

    // NEAR Node URL (check if explicitly set)
    let near_node_url_explicit = args.near_node_url.is_some() || env::var("NEAR_NODE_URL").is_ok();
    let near_node_url = args
        .near_node_url
        .or_else(|| env::var("NEAR_NODE_URL").ok())
        .unwrap_or_else(|| "https://rpc.mainnet.fastnear.com/".to_string()); // Default to mainnet

    // Validate URLs
    validate_url(&near_node_url, "NEAR_NODE_URL")?;

    // Archival RPC URL (optional, validate if provided)
    let archival_rpc_url = args
        .archival_rpc_url
        .or_else(|| env::var("ARCHIVAL_RPC_URL").ok());
    if let Some(ref url) = archival_rpc_url {
        validate_url(url, "ARCHIVAL_RPC_URL")?;
    }

    let fastnear_api_url = args
        .fastnear_api_url
        .or_else(|| env::var("FASTNEAR_API_URL").ok())
        .unwrap_or_else(|| "https://tx.main.fastnear.com".to_string());
    validate_url(&fastnear_api_url, "FASTNEAR_API_URL")?;

    let ws_url = args
        .ws_url
        .or_else(|| env::var("WS_URL").ok())
        .unwrap_or_else(|| "ws://127.0.0.1:63736".to_string());
    validate_url(&ws_url, "WS_URL")?;

    // FPS choices with validation
    let render_fps_choices = args
        .render_fps_choices
        .or_else(|| env::var("RENDER_FPS_CHOICES").ok())
        .map(|s| parse_fps_list(&s))
        .unwrap_or_else(|| vec![20, 30, 60]);

    // Ensure render_fps_choices is not empty
    if render_fps_choices.is_empty() {
        return Err(anyhow!(
            "RENDER_FPS_CHOICES must contain at least one valid value (1-120)"
        ));
    }

    // Render FPS (default to first choice if not specified)
    let default_fps = *render_fps_choices.first().unwrap();
    let render_fps = args
        .render_fps
        .or_else(|| env::var("RENDER_FPS").ok().and_then(|s| s.parse().ok()))
        .unwrap_or(default_fps);
    let render_fps = validate_in_range(render_fps, 1, 120, "RENDER_FPS")?;

    // Parse and validate RPC settings
    let poll_interval_ms = args
        .poll_interval_ms
        .or_else(|| {
            env::var("POLL_INTERVAL_MS")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(1000);
    let poll_interval_ms = validate_in_range(poll_interval_ms, 100, 10000, "POLL_INTERVAL_MS")?;

    let poll_max_catchup = args
        .poll_max_catchup
        .or_else(|| {
            env::var("POLL_MAX_CATCHUP")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(5);
    let poll_max_catchup = validate_in_range(poll_max_catchup, 1, 100, "POLL_MAX_CATCHUP")?;

    let poll_chunk_concurrency = args
        .poll_chunk_concurrency
        .or_else(|| {
            env::var("POLL_CHUNK_CONCURRENCY")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(4);
    let poll_chunk_concurrency =
        validate_in_range(poll_chunk_concurrency, 1, 16, "POLL_CHUNK_CONCURRENCY")?;

    let rpc_timeout_ms = args
        .rpc_timeout_ms
        .or_else(|| env::var("RPC_TIMEOUT_MS").ok().and_then(|s| s.parse().ok()))
        .unwrap_or(8000);
    let rpc_timeout_ms = validate_in_range(rpc_timeout_ms, 1000, 60000, "RPC_TIMEOUT_MS")?;

    let rpc_retries = args
        .rpc_retries
        .or_else(|| env::var("RPC_RETRIES").ok().and_then(|s| s.parse().ok()))
        .unwrap_or(2);
    let rpc_retries = validate_in_range(rpc_retries, 0, 10, "RPC_RETRIES")?;

    let keep_blocks = args
        .keep_blocks
        .or_else(|| env::var("KEEP_BLOCKS").ok().and_then(|s| s.parse().ok()))
        .unwrap_or(100);
    let keep_blocks = validate_in_range(keep_blocks, 10, 10000, "KEEP_BLOCKS")?;

    // Build default filter with priority: WATCH_ACCOUNTS > DEFAULT_FILTER > default
    let default_filter = if let Some(watch_accounts) = args
        .watch_accounts
        .or_else(|| env::var("WATCH_ACCOUNTS").ok())
    {
        // Parse comma-separated account list and build filter
        if watch_accounts.is_empty() {
            String::new()
        } else {
            watch_accounts
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|account| format!("acct:{account}"))
                .collect::<Vec<_>>()
                .join(" ")
        }
    } else {
        args.default_filter
            .or_else(|| env::var("DEFAULT_FILTER").ok())
            .unwrap_or_else(|| "acct:intents.near".to_string())
    };

    // Use default theme (theme selection not implemented yet)
    let theme = crate::theme::Theme::default();

    // Build and return config
    Ok(Config {
        source,
        ws_url,
        ws_fetch_blocks: args
            .ws_fetch_blocks
            .or_else(|| {
                env::var("WS_FETCH_BLOCKS")
                    .ok()
                    .map(|s| s.to_lowercase() == "true")
            })
            .unwrap_or(true),
        render_fps,
        render_fps_choices,
        poll_interval_ms,
        poll_max_catchup,
        poll_chunk_concurrency,
        keep_blocks,
        near_node_url,
        near_node_url_explicit,
        archival_rpc_url,
        fastnear_api_url,
        rpc_timeout_ms,
        rpc_retries,
        fastnear_auth_token: args.fastnear_auth_token.or_else(fastnear_env_token),
        default_filter,
        theme,
    })
}

/// Validate URL format (basic check)
fn validate_url(url: &str, name: &str) -> Result<()> {
    if url.is_empty() {
        return Err(anyhow!("{name} cannot be empty"));
    }

    // Basic scheme validation
    if url.starts_with("ws://")
        || url.starts_with("wss://")
        || url.starts_with("http://")
        || url.starts_with("https://")
    {
        Ok(())
    } else {
        Err(anyhow!(
            "{name} must start with ws://, wss://, http://, or https://"
        ))
    }
}

/// Cross-target FastNEAR key resolution.
/// Priority: FASTNEAR_API_KEY (canonical) -> FASTNEAR_AUTH_TOKEN (legacy alias).
/// Avoids runtime env reads on WASM (which cause panics).
pub fn fastnear_env_token() -> Option<String> {
    // Native & proxy: read at runtime.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let api_key = std::env::var("FASTNEAR_API_KEY")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if api_key.is_some() {
            return api_key;
        }

        std::env::var("FASTNEAR_AUTH_TOKEN")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    // Web/Tauri (wasm): bake token at compile time if available.
    #[cfg(target_arch = "wasm32")]
    {
        option_env!("FASTNEAR_API_KEY")
            .or(option_env!("FASTNEAR_AUTH_TOKEN"))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }
}

/// Cross-target helper returning empty string when no key is configured.
pub fn fastnear_token() -> String {
    fastnear_env_token().unwrap_or_default()
}

/// Append FastNEAR API key as a query parameter in the documented form.
pub fn with_fastnear_api_key(url: &str, key: Option<&str>) -> String {
    let Some(key) = key.map(str::trim).filter(|s| !s.is_empty()) else {
        return url.to_string();
    };
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}apiKey={}", urlencoding::encode(key))
}

/// Print current configuration (useful for debugging)
impl Config {
    #[allow(dead_code)]
    pub fn print_summary(&self) {
        eprintln!("NEARx Configuration:");
        eprintln!("  Source: {}", self.source);
        match self.source {
            Source::Ws => {
                eprintln!("  WebSocket URL: {}", self.ws_url);
                eprintln!("  Fetch Blocks: {}", self.ws_fetch_blocks);
            }
            Source::Rpc => {
                eprintln!("  RPC URL: {}", self.near_node_url);
                eprintln!("  Poll Interval: {}ms", self.poll_interval_ms);
                eprintln!("  Max Catchup: {} blocks", self.poll_max_catchup);
                eprintln!("  Chunk Concurrency: {}", self.poll_chunk_concurrency);
                eprintln!("  RPC Timeout: {}ms", self.rpc_timeout_ms);
                eprintln!("  RPC Retries: {}", self.rpc_retries);
            }
        }
        eprintln!("  Render FPS: {}", self.render_fps);
        eprintln!("  Keep Blocks: {}", self.keep_blocks);
        if self.fastnear_auth_token.is_some() {
            eprintln!("  FastNEAR Auth: Configured");
        }
    }
}
