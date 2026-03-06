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

mod broker;
mod config;
mod credentials;
mod hardware_wallet;
mod keychain;
mod rpc;
mod settings;
mod signing;
mod socket;
mod tests;
mod token;
mod user_presence;
mod util;

use std::sync::Arc;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    #[cfg(unix)]
    {
        let state = Arc::new(broker::BrokerState::default());
        if let Err(e) = socket::run_unix(state) {
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
