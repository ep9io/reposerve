//! The process: bind a socket, watch the files and serve them.

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tokio::net::TcpListener;
use tokio::sync::broadcast;

use crate::config::Config;
use crate::error::Error;
use crate::site::index::RepoIndex;
use crate::web::cache::PageCache;
use crate::web::routes;

/// How many messages the reload channel holds for a slow reader.
const RELOAD_BUFFER: usize = 16;

/// How long the watcher waits for a burst of writes to settle, in milliseconds.
const SETTLE: Duration = Duration::from_millis(200);

/// How often the fallback watcher looks for a change, in seconds.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// What every request handler shares.
#[derive(Debug)]
pub struct AppState {
    /// The resolved configuration.
    pub config: Config,
    /// Signals a browser which paths changed.
    pub reload: broadcast::Sender<String>,
    /// The current file list, replaced when a file changes.
    index: RwLock<Arc<RepoIndex>>,
    /// Rendered pages, dropped when a file changes.
    pub pages: PageCache,
}

impl AppState {
    /// Builds the shared state and loads the file list.
    ///
    /// # Errors
    ///
    /// Fails when the root cannot be listed.
    pub fn new(config: Config) -> Result<Self, Error> {
        let index = RepoIndex::load(&config)?;
        let (reload, _) = broadcast::channel(RELOAD_BUFFER);
        Ok(Self {
            config,
            reload,
            index: RwLock::new(Arc::new(index)),
            pages: PageCache::default(),
        })
    }

    /// The current file list.
    #[must_use]
    pub fn index(&self) -> Arc<RepoIndex> {
        match self.index.read() {
            Ok(index) => Arc::clone(&index),
            Err(poisoned) => Arc::clone(&poisoned.into_inner()),
        }
    }

    /// Reloads the file list from disk.
    ///
    /// # Errors
    ///
    /// Fails when the root cannot be listed. The old list stays in place.
    pub fn refresh(&self) -> Result<(), Error> {
        let index = Arc::new(RepoIndex::load(&self.config)?);
        match self.index.write() {
            Ok(mut current) => *current = index,
            Err(poisoned) => *poisoned.into_inner() = index,
        }
        self.pages.clear();
        Ok(())
    }

    /// The number of files the repository holds.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.index().total()
    }
}

/// A server with a bound socket, ready to run.
#[derive(Debug)]
pub struct Server {
    state: Arc<AppState>,
    listener: TcpListener,
}

impl Server {
    /// Validates the configuration, binds the socket and starts watching.
    ///
    /// # Errors
    ///
    /// Fails when the configuration is not valid, when the root cannot be
    /// listed, or when the address is in use.
    pub async fn bind(config: Config) -> Result<Self, Error> {
        config.validate()?;
        let address = format!("{}:{}", config.bind, config.port);
        let listener = TcpListener::bind(&address)
            .await
            .map_err(|source| Error::Bind { address, source })?;

        let state = Arc::new(AppState::new(config)?);
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

    /// Whether the configuration asks for a browser at start.
    #[must_use]
    pub fn wants_browser(&self) -> bool {
        self.state.config.open
    }

    /// Runs until the process stops.
    ///
    /// # Errors
    ///
    /// Fails when the transport stops.
    pub async fn serve(self) -> Result<(), Error> {
        axum::serve(self.listener, routes::router(Arc::clone(&self.state)))
            .with_graceful_shutdown(shutdown_signal())
            .await
            .map_err(|source| Error::Serve { source })
    }

    /// Prints the addresses a reader should open and returns them.
    pub fn report(&self) -> Vec<String> {
        let config = &self.state.config;
        println!(
            "Serving {} files from {}",
            self.state.file_count(),
            config.root.display()
        );
        let mut urls = Vec::new();
        if matches!(config.bind.as_str(), "0.0.0.0" | "::" | "") {
            if let Some(address) = network_address() {
                urls.push(format!("http://{address}:{}/", config.port));
            }
            urls.push(format!("http://127.0.0.1:{}/", config.port));
        } else {
            urls.push(format!("http://{}:{}/", config.bind, config.port));
        }
        for url in &urls {
            match config.token.as_deref() {
                Some(token) => println!("  {url}?token={token}"),
                None => println!("  {url}"),
            }
        }
        urls
    }
}

/// Watches the root and tells the browsers when a path changes.
///
/// The `notify` crate uses the platform's own mechanism, so an edit is noticed
/// at once. A filesystem that cannot report changes falls back to a poll every
/// few seconds, which costs one listing pass.
fn watch(state: Arc<AppState>) {
    tokio::task::spawn_blocking(move || {
        let (sender, receiver) = std::sync::mpsc::channel::<PathBuf>();
        let mut watcher =
            match notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else {
                    return;
                };
                // A read is not a change. Without this filter, serving a file
                // would tell the browser to reload, and the reload would read
                // the file again.
                if !affects_content(&event.kind) {
                    return;
                }
                for path in event.paths {
                    if is_ignored(&path) {
                        continue;
                    }
                    let _ = sender.send(path);
                }
            }) {
                Ok(watcher) => watcher,
                Err(error) => {
                    eprintln!("warning: cannot watch for changes: {error}");
                    return;
                }
            };
        if let Err(error) = watcher.watch(&state.config.root, RecursiveMode::Recursive) {
            eprintln!(
                "warning: cannot watch {}: {error}",
                state.config.root.display()
            );
            poll(state);
            return;
        }
        let mut batch: Vec<String> = Vec::new();
        // The watcher reports absolute paths, so compare against an absolute
        // root even when the caller gave a relative one.
        let root =
            std::fs::canonicalize(&state.config.root).unwrap_or_else(|_| state.config.root.clone());
        while let Ok(path) = receiver.recv() {
            batch.push(relative(&root, &path));
            // Collect the rest of a burst, so one save yields one reload.
            while let Ok(path) = receiver.recv_timeout(SETTLE) {
                batch.push(relative(&root, &path));
            }
            batch.sort();
            batch.dedup();
            announce(&state, &batch);
            batch.clear();
        }
    });
}

/// The fallback watcher, for a filesystem that reports no events.
fn poll(state: Arc<AppState>) {
    let mut previous = fingerprint(&state);
    loop {
        std::thread::sleep(POLL_INTERVAL);
        let current = fingerprint(&state);
        if current == previous {
            continue;
        }
        previous = current;
        announce(&state, &[]);
    }
}

/// A fingerprint of the served paths with their size and modification time.
fn fingerprint(state: &AppState) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let index = state.index();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for path in index.paths() {
        path.hash(&mut hasher);
        if let Ok(metadata) = std::fs::metadata(state.config.root.join(path)) {
            metadata.len().hash(&mut hasher);
            if let Ok(modified) = metadata.modified() {
                modified.hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

/// Reloads the file list, drops the page cache and tells every browser.
fn announce(state: &AppState, paths: &[String]) {
    if let Err(error) = state.refresh() {
        eprintln!("warning: cannot reload the file list: {error}");
        return;
    }
    let message = serde_json::json!({ "paths": paths }).to_string();
    // No receivers means no browser is listening, which is normal.
    let _ = state.reload.send(message);
}

/// The repository-relative form of a watched path.
///
/// A root such as `./showcase` arrives with a `.` component, and so does the
/// path the watcher reports. Dropping those components keeps the comparison
/// working without a canonicalise call, which would fail for a deleted file.
fn relative(root: &Path, path: &Path) -> String {
    let root: PathBuf = root.components().collect();
    let path: PathBuf = path.components().collect();
    path.strip_prefix(&root)
        .unwrap_or(&path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Whether an event can change what a page shows.
///
/// A read is not a change, and a file's permissions alone are not either. The
/// three kinds below cover an edit, a new file and a deleted file.
fn affects_content(kind: &notify::EventKind) -> bool {
    kind.is_create() || kind.is_modify() || kind.is_remove()
}

/// Whether a path belongs to a tool, not to the served content.
fn is_ignored(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some(".git" | "target" | "node_modules")
        )
    })
}

/// Resolves when the process is asked to stop.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
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

    #[test]
    fn a_state_loads_its_index_and_can_refresh_it() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        std::fs::write(directory.path().join("note.md"), "first").expect("the file is written");
        let config = Config {
            root: directory.path().to_path_buf(),
            ..Config::default()
        };
        let state = AppState::new(config).expect("the state is built");
        assert_eq!(state.file_count(), 1);

        std::fs::write(directory.path().join("second.md"), "second").expect("the file is written");
        state.refresh().expect("the list reloads");
        assert_eq!(state.file_count(), 2);
    }

    #[test]
    fn a_watched_path_becomes_a_repository_path() {
        let root = Path::new("/tmp/docs");
        assert_eq!(relative(root, Path::new("/tmp/docs/a/b.md")), "a/b.md");
        assert_eq!(
            relative(Path::new("./docs"), Path::new("./docs/a.md")),
            "a.md",
            "a relative root and path still compare"
        );
        assert_eq!(
            relative(Path::new("/tmp/./docs"), Path::new("/tmp/docs/./a.md")),
            "a.md",
            "a dot component does not defeat the comparison"
        );
    }

    #[test]
    fn a_read_is_not_a_change() {
        use notify::event::{AccessKind, CreateKind, EventKind, ModifyKind, RemoveKind};
        assert!(!affects_content(&EventKind::Access(AccessKind::Close(
            notify::event::AccessMode::Read
        ))));
        assert!(affects_content(&EventKind::Create(CreateKind::File)));
        assert!(affects_content(&EventKind::Modify(ModifyKind::Any)));
        assert!(affects_content(&EventKind::Remove(RemoveKind::File)));
    }

    #[test]
    fn a_tool_directory_is_ignored() {
        assert!(is_ignored(Path::new("/repo/.git/index")));
        assert!(is_ignored(Path::new("/repo/target/debug/x")));
        assert!(is_ignored(Path::new("/repo/node_modules/a/b.js")));
        assert!(!is_ignored(Path::new("/repo/docs/guide.md")));
    }
}
