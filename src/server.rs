//! The process: bind a socket, watch the files and serve them.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::error::Error;
use crate::index::RepoIndex;
use crate::routes;

/// How often the watcher looks for a change, in seconds.
const WATCH_INTERVAL: u64 = 1;

/// What every request handler shares.
#[derive(Debug)]
pub struct AppState {
    /// The resolved configuration.
    pub config: Config,
    /// Signals a browser that a file changed.
    pub reload: broadcast::Sender<()>,
}

/// A server with a bound socket, ready to run.
#[derive(Debug)]
pub struct Server {
    state: Arc<AppState>,
    listener: TcpListener,
}

impl Server {
    /// Validates the configuration and binds the socket.
    ///
    /// # Errors
    ///
    /// Fails when the configuration is not valid, or when the address is in use.
    pub async fn bind(config: Config) -> Result<Self, Error> {
        config.validate()?;
        let address = format!("{}:{}", config.bind, config.port);
        let listener = TcpListener::bind(&address)
            .await
            .map_err(|source| Error::Bind { address, source })?;

        let (reload, _) = broadcast::channel(8);
        let state = Arc::new(AppState { config, reload });
        if state.config.live_reload {
            watch(Arc::clone(&state));
        }
        Ok(Self { state, listener })
    }

    /// The address the socket is bound to.
    ///
    /// # Errors
    ///
    /// Fails when the operating system cannot report the address.
    pub fn local_addr(&self) -> Result<SocketAddr, Error> {
        self.listener.local_addr().map_err(|source| Error::Bind {
            address: "the bound socket".to_string(),
            source,
        })
    }

    /// Runs until the process stops.
    ///
    /// # Errors
    ///
    /// Fails when the transport stops.
    pub async fn serve(self) -> Result<(), Error> {
        axum::serve(self.listener, routes::router(Arc::clone(&self.state)))
            .await
            .map_err(|source| Error::Serve { source })
    }

    /// Prints the addresses a reader should open.
    pub fn report(&self) {
        let config = &self.state.config;
        let count = RepoIndex::load(config).map_or(0, |index| index.total());
        println!("Serving {count} files from {}", config.root.display());
        if matches!(config.bind.as_str(), "0.0.0.0" | "::" | "") {
            if let Some(address) = network_address() {
                println!("  http://{address}:{}/", config.port);
            }
            println!("  http://127.0.0.1:{}/", config.port);
        } else {
            println!("  http://{}:{}/", config.bind, config.port);
        }
    }
}

/// Sends a reload event whenever a served file changes.
///
/// Polling is deliberate. It needs no platform watcher, it covers new files as
/// well as edited ones, and one pass over a documentation tree costs a few
/// milliseconds each second.
fn watch(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut previous = snapshot(&state.config).await;
        loop {
            tokio::time::sleep(Duration::from_secs(WATCH_INTERVAL)).await;
            let current = snapshot(&state.config).await;
            if current.is_some() && current != previous {
                previous = current;
                // No receivers means no browser is listening, which is normal.
                let _ = state.reload.send(());
            }
        }
    });
}

/// A hash of every served path with its size and modification time.
async fn snapshot(config: &Config) -> Option<u64> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || {
        let index = RepoIndex::load(&config).ok()?;
        let mut hasher = DefaultHasher::new();
        for path in index.paths() {
            path.hash(&mut hasher);
            if let Ok(metadata) = std::fs::metadata(config.root.join(path)) {
                metadata.len().hash(&mut hasher);
                if let Ok(modified) = metadata.modified() {
                    modified.hash(&mut hasher);
                }
            }
        }
        Some(hasher.finish())
    })
    .await
    .ok()?
}

/// The address this host uses to reach the rest of the network.
///
/// Connecting a UDP socket sends no packet, but it makes the kernel choose an
/// interface and therefore an address. This returns `None` when the host has no
/// route at all, and the caller then prints the loopback address alone.
fn network_address() -> Option<IpAddr> {
    for target in ["192.0.2.1:9", "8.8.8.8:80", "10.255.255.255:9"] {
        let Ok(socket) = UdpSocket::bind("0.0.0.0:0") else {
            continue;
        };
        if socket.connect(target).is_err() {
            continue;
        }
        if let Ok(SocketAddr::V4(address)) = socket.local_addr() {
            let ip = *address.ip();
            if ip != Ipv4Addr::LOCALHOST && !ip.is_loopback() {
                return Some(IpAddr::V4(ip));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_snapshot_changes_when_a_file_changes() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(directory.path().join("note.md"), "first").expect("the file is written");
        let config = Config {
            root: directory.path().to_path_buf(),
            ..Config::default()
        };

        let before = snapshot(&config).await.expect("a snapshot");
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(directory.path().join("note.md"), "second").expect("the file is rewritten");
        let after = snapshot(&config).await.expect("a snapshot");

        assert_ne!(before, after, "a changed file must change the fingerprint");
    }

    #[tokio::test]
    async fn a_snapshot_does_not_change_on_its_own() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(directory.path().join("note.md"), "text").expect("the file is written");
        let config = Config {
            root: directory.path().to_path_buf(),
            ..Config::default()
        };

        assert_eq!(snapshot(&config).await, snapshot(&config).await);
    }
}
