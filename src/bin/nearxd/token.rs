use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::keychain::{keychain_delete_generic, keychain_read_generic, keychain_write_generic};

#[cfg(target_os = "macos")]
pub(crate) const KEYCHAIN_SERVICE: &str = "nearxd.fastnear.auth";
#[cfg(target_os = "macos")]
pub(crate) const KEYCHAIN_ACCOUNT: &str = "fastnear_auth_token";

pub(crate) trait TokenStore: Send + Sync + std::fmt::Debug {
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

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct MacKeychainTokenStore;

#[cfg(target_os = "macos")]
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

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct AutoTokenStore {
    primary: Arc<dyn TokenStore>,
    fallback: Arc<dyn TokenStore>,
}

#[cfg(target_os = "macos")]
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

pub(crate) fn build_token_store() -> Arc<dyn TokenStore> {
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
        use std::os::unix::fs::PermissionsExt;
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
fn read_keychain_token() -> Option<String> {
    keychain_read_generic(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
}

#[cfg(target_os = "macos")]
fn persist_keychain_token(token: &str) -> Result<(), String> {
    keychain_write_generic(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT, token)
}

#[cfg(target_os = "macos")]
fn clear_keychain_token() -> Result<(), String> {
    keychain_delete_generic(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
}
