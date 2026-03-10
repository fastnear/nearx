use interprocess::local_socket::{
    prelude::*, GenericFilePath, GenericNamespaced, Listener, ListenerOptions, Stream,
};
use std::env;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrokerEndpoint {
    Filesystem(PathBuf),
    Namespaced(String),
}

impl BrokerEndpoint {
    pub fn from_env() -> Self {
        if let Ok(raw) = env::var("NEARXD_ENDPOINT") {
            if let Some(endpoint) = Self::parse(raw.trim()) {
                return endpoint;
            }
        }
        #[cfg(unix)]
        if let Ok(raw) = env::var("NEARXD_SOCKET_PATH") {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return Self::Filesystem(PathBuf::from(trimmed));
            }
        }
        Self::default_endpoint()
    }

    pub fn default_endpoint() -> Self {
        #[cfg(windows)]
        {
            Self::Namespaced("nearxd".to_string())
        }
        #[cfg(not(windows))]
        {
            Self::Filesystem(env::temp_dir().join("nearxd.sock"))
        }
    }

    pub fn tauri_sidecar_endpoint(pid: u32) -> Self {
        #[cfg(windows)]
        {
            Self::Namespaced(format!("nearxd-tauri-{pid}"))
        }
        #[cfg(not(windows))]
        {
            Self::Filesystem(env::temp_dir().join(format!("nearxd-tauri-{pid}.sock")))
        }
    }

    pub fn display(&self) -> String {
        match self {
            Self::Filesystem(path) => path.display().to_string(),
            Self::Namespaced(name) => {
                #[cfg(windows)]
                {
                    format!(r"\\.\pipe\{name}")
                }
                #[cfg(not(windows))]
                {
                    name.clone()
                }
            }
        }
    }

    pub fn export_env_vars(&self) -> Vec<(&'static str, String)> {
        let mut out = vec![("NEARXD_ENDPOINT", self.to_env_string())];
        if let Self::Filesystem(path) = self {
            out.push(("NEARXD_SOCKET_PATH", path.display().to_string()));
        }
        out
    }

    pub fn connect(&self) -> io::Result<Stream> {
        match self {
            Self::Filesystem(path) => Stream::connect(path_to_name(path)?),
            Self::Namespaced(name) => Stream::connect(name_to_name(name)?),
        }
    }

    pub fn bind(&self) -> io::Result<Listener> {
        match self {
            Self::Filesystem(path) => ListenerOptions::new()
                .name(path_to_name(path)?)
                .reclaim_name(true)
                .create_sync(),
            Self::Namespaced(name) => ListenerOptions::new()
                .name(name_to_name(name)?)
                .create_sync(),
        }
    }

    pub fn filesystem_path(&self) -> Option<&Path> {
        match self {
            Self::Filesystem(path) => Some(path.as_path()),
            Self::Namespaced(_) => None,
        }
    }

    fn to_env_string(&self) -> String {
        match self {
            Self::Filesystem(path) => format!("unix:{}", path.display()),
            Self::Namespaced(name) => format!("name:{name}"),
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        if raw.is_empty() {
            return None;
        }
        if let Some(rest) = raw.strip_prefix("unix:") {
            let trimmed = rest.trim();
            return (!trimmed.is_empty()).then(|| Self::Filesystem(PathBuf::from(trimmed)));
        }
        if let Some(rest) = raw.strip_prefix("name:") {
            let trimmed = rest.trim();
            return (!trimmed.is_empty()).then(|| Self::Namespaced(trimmed.to_string()));
        }
        #[cfg(unix)]
        if raw.starts_with('/') || raw.starts_with("./") || raw.starts_with("../") {
            return Some(Self::Filesystem(PathBuf::from(raw)));
        }
        Some(Self::Namespaced(raw.to_string()))
    }
}

fn path_to_name(path: &Path) -> io::Result<interprocess::local_socket::Name<'static>> {
    path.to_path_buf().to_fs_name::<GenericFilePath>()
}

fn name_to_name(name: &str) -> io::Result<interprocess::local_socket::Name<'static>> {
    name.to_string().to_ns_name::<GenericNamespaced>()
}

#[cfg(test)]
mod tests {
    use super::BrokerEndpoint;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn with_env<T>(vars: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
        let _guard = ENV_LOCK.lock().expect("env lock");
        let previous = vars
            .iter()
            .map(|(key, _)| ((*key).to_string(), std::env::var(key).ok()))
            .collect::<Vec<_>>();
        for (key, value) in vars {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        let result = f();
        for (key, value) in previous {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        result
    }

    #[test]
    fn export_env_vars_keeps_legacy_unix_socket_path() {
        let endpoint =
            BrokerEndpoint::Filesystem(std::path::PathBuf::from("/tmp/nearxd-test.sock"));
        let env = endpoint.export_env_vars();
        assert!(
            env.iter()
                .any(|(key, value)| *key == "NEARXD_ENDPOINT"
                    && value == "unix:/tmp/nearxd-test.sock")
        );
        assert!(env
            .iter()
            .any(|(key, value)| *key == "NEARXD_SOCKET_PATH" && value == "/tmp/nearxd-test.sock"));
    }

    #[cfg(unix)]
    #[test]
    fn from_env_prefers_explicit_endpoint_over_legacy_socket_path() {
        with_env(
            &[
                ("NEARXD_ENDPOINT", Some("unix:/tmp/explicit.sock")),
                ("NEARXD_SOCKET_PATH", Some("/tmp/legacy.sock")),
            ],
            || {
                assert_eq!(
                    BrokerEndpoint::from_env(),
                    BrokerEndpoint::Filesystem(std::path::PathBuf::from("/tmp/explicit.sock")),
                );
            },
        );
    }

    #[test]
    fn default_endpoint_matches_platform() {
        let endpoint = BrokerEndpoint::default_endpoint();
        #[cfg(windows)]
        assert!(matches!(endpoint, BrokerEndpoint::Namespaced(_)));
        #[cfg(not(windows))]
        assert!(matches!(endpoint, BrokerEndpoint::Filesystem(_)));
    }
}
