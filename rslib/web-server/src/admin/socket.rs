// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! The server's end of the admin socket, and the client's. One JSON request per
//! connection, followed by one JSON response. A Unix socket, as this must never
//! be reachable from the network; docs/admin-cli.md has the reasoning.

use std::fs;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use serde_json::json;
use serde_json::Value;
use tokio::net::UnixListener;
use tokio::task::JoinHandle;

use super::Admin;

/// Requests name files rather than containing them, so are small.
const MAX_REQUEST_BYTES: u64 = 16 * 1024 * 1024;
/// A client writes its request at once. This stops one that doesn't from
/// holding up shutdown.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// A sun_path holds 108 bytes on Linux.
const MAX_DIRECT_PATH: usize = 100;

/// `collection.admin.sock`, for `collection.anki2`.
pub fn socket_path(collection: &Path) -> PathBuf {
    collection.with_extension("admin.sock")
}

/// Run `action` with a path to `path` that is short enough for a socket. A
/// deep folder would otherwise be refused, whereas the folder's file descriptor
/// always gives a short name for it.
fn with_short_path<T>(path: &Path, action: impl FnOnce(&Path) -> std::io::Result<T>) -> Result<T> {
    if path.as_os_str().len() <= MAX_DIRECT_PATH {
        return Ok(action(path)?);
    }
    let parent = path.parent().context("no parent")?;
    let name = path.file_name().context("no file name")?;
    let folder = fs::File::open(parent).with_context(|| parent.display().to_string())?;
    let short = PathBuf::from(format!("/proc/self/fd/{}", folder.as_raw_fd())).join(name);
    Ok(action(&short)?)
}

fn connect(path: &Path) -> Result<UnixStream> {
    with_short_path(path, |path| UnixStream::connect(path))
}

pub struct AdminSocket {
    path: PathBuf,
    accepting: JoinHandle<()>,
}

impl AdminSocket {
    /// Listen beside the collection. The socket is private from the moment it
    /// has its final name, as a private folder holds it until then.
    pub async fn start(admin: Arc<Admin>) -> Result<Self> {
        let path = socket_path(admin.collection());
        remove_if_stale(&path)?;
        let listener = bind_private(&path)?;
        let accepting = tokio::spawn(accept_loop(listener, admin));
        Ok(Self { path, accepting })
    }

    /// Stop taking commands, and remove the socket so that tools open the
    /// collection directly from now on. Running commands get to finish, as
    /// closing the collection waits for them.
    pub fn stop(self) {
        self.accepting.abort();
        let _ = fs::remove_file(&self.path);
    }
}

fn remove_if_stale(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    if !metadata.file_type().is_socket() {
        bail!(
            "{} exists and isn't a socket; not touching it",
            path.display()
        );
    }
    if connect(path).is_ok() {
        bail!(
            "another server is using this collection: {}",
            path.display()
        );
    }
    fs::remove_file(path).with_context(|| path.display().to_string())
}

fn bind_private(path: &Path) -> Result<UnixListener> {
    let parent = path.parent().context("socket path has no parent")?;
    let name = path.file_name().context("socket path has no name")?;
    let private = parent.join(format!(".admin-sock-{}", std::process::id()));
    fs::create_dir_all(&private).with_context(|| private.display().to_string())?;
    let result = (|| {
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
        let temporary = private.join(name);
        let listener = with_short_path(&temporary, |path| {
            std::os::unix::net::UnixListener::bind(path)
        })
        .with_context(|| format!("couldn't listen on {}", temporary.display()))?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
        fs::rename(&temporary, path)?;
        listener.set_nonblocking(true)?;
        Ok(UnixListener::from_std(listener)?)
    })();
    let _ = fs::remove_dir(&private);
    result
}

async fn accept_loop(listener: UnixListener, admin: Arc<Admin>) {
    // SAFETY net for the file permissions rather than the main control
    let own_uid = rustix::process::geteuid().as_raw();
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(err) => {
                tracing::error!("admin socket: {err}");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        if stream.peer_cred().map(|peer| peer.uid()).ok() != Some(own_uid) {
            tracing::warn!("admin socket: refused a connection from another user");
            continue;
        }
        let admin = admin.clone();
        // backend calls block, and an operation may take minutes
        tokio::task::spawn_blocking(move || {
            let Ok(stream) = stream.into_std() else {
                return;
            };
            if let Err(err) = serve_connection(stream, &admin) {
                tracing::info!("admin socket: {err:#}");
            }
        });
    }
}

fn serve_connection(mut stream: UnixStream, admin: &Admin) -> Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(REQUEST_TIMEOUT))?;
    let mut request = vec![];
    (&stream)
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut request)?;
    let response = if request.len() as u64 > MAX_REQUEST_BYTES {
        json!({"ok": false, "error": "request too large"})
    } else {
        match serde_json::from_slice::<super::Request>(&request) {
            Ok(request) => admin.execute(&request),
            Err(err) => json!({"ok": false, "error": format!("bad request: {err}")}),
        }
    };
    stream.write_all(response.to_string().as_bytes())?;
    Ok(())
}

/// The outcome of trying to reach a running server.
pub enum Sent {
    Response(Value),
    /// Nothing's listening, so the collection may be opened directly.
    NoServer,
}

/// Send a request, if a server has this collection.
pub fn send(collection: &Path, request: &super::Request) -> Result<Sent> {
    let path = socket_path(collection);
    let mut stream = match connect(&path) {
        Ok(stream) => stream,
        Err(err) => {
            let io = err.downcast_ref::<std::io::Error>().map(|e| e.kind());
            return match io {
                // no socket, or left behind by a crashed server
                Some(ErrorKind::NotFound | ErrorKind::ConnectionRefused) => Ok(Sent::NoServer),
                _ => Err(err.context(format!("couldn't connect to {}", path.display()))),
            };
        }
    };
    stream.write_all(serde_json::to_string(request)?.as_bytes())?;
    stream.shutdown(std::net::Shutdown::Write)?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    if response.is_empty() {
        return Err(anyhow!(
            "the server closed the connection without answering; it may be stopping"
        ));
    }
    Ok(Sent::Response(serde_json::from_str(&response)?))
}
