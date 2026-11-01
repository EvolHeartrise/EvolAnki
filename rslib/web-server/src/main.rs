// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! A single-user web server around Anki's Rust backend, with no login, for
//! one's own network. See docs/web-server.md.

mod files;
mod frame;
mod guard;
mod seed;

use std::future::IntoFuture;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::net::UdpSocket;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anki::backend::Backend;
use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use axum::body::Bytes;
use axum::extract::Request;
use axum::extract::State;
use axum::http::header;
use axum::http::Method;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::Router;
use clap::Parser;
use evolanki_server::admin::socket::AdminSocket;
use evolanki_server::admin::Admin;
use evolanki_server::backups;
use evolanki_server::backups::Backups;
use evolanki_server::collection::close_collection;
use evolanki_server::collection::open_collection;
pub(crate) use evolanki_server::rpc;
use tokio::net::TcpListener;
use tokio::sync::watch;

/// Enough for any page's request; media uploads will want a dedicated route.
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
/// How long open connections get to finish once shutdown starts.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

#[derive(Parser)]
#[command(about = "Serve a collection to browsers on this network, with no login")]
struct Args {
    /// Path to the collection (.anki2). Media lives in the matching .media
    /// folder, as in a desktop profile.
    #[arg(long)]
    collection: PathBuf,
    /// Port to listen on. 0 picks a free one.
    #[arg(long, default_value_t = 8765)]
    port: u16,
    /// IP address to listen on. The default is every interface, so that a phone
    /// can reach it; 127.0.0.1 keeps it to this machine. There is no login.
    #[arg(long, default_value = "0.0.0.0")]
    host: IpAddr,
    /// Another name this server may be reached by, such as a VPN's, or a public
    /// IP. Can be given more than once. Loopback, private IP addresses and this
    /// machine's name, with or without .local, are always accepted.
    #[arg(long = "allow-host", value_name = "NAME")]
    allow_hosts: Vec<String>,
    /// Folder of built web assets, containing sveltekit/index.html. By default,
    /// out/web in the build tree this binary lives in, wherever it's run from.
    #[arg(long)]
    web_root: Option<PathBuf>,
    /// Least seconds between backups while the collection is changing.
    #[arg(long, default_value_t = 1800, value_parser = clap::value_parser!(u64).range(1..))]
    backup_interval_secs: u64,
    /// Create the collection if it doesn't exist. Without this, a mistyped
    /// path is an error rather than a new empty collection.
    #[arg(long)]
    create: bool,
    /// When creating a collection, add sample notes and reviews to it.
    #[arg(long, requires = "create")]
    seed_demo: bool,
    /// When creating a collection, add the decks and notes in this JSON file.
    /// For tests; see SeedFile in seed.rs.
    #[arg(long, requires = "create", conflicts_with = "seed_demo")]
    seed_file: Option<PathBuf>,
}

pub struct AppState {
    backend: Backend,
    web_root: PathBuf,
    media_dir: PathBuf,
    hosts: guard::AllowedHosts,
    /// Random path prefix for what sandboxed frames may load; see frame.rs.
    frame_token: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    anki::log::set_global_logger(None)?;
    run(Args::parse()).await
}

async fn run(args: Args) -> Result<()> {
    let web_root = match args.web_root {
        Some(path) => path,
        None => default_web_root()?,
    };
    check_web_root(&web_root)?;

    let existed = args.collection.exists();
    if !existed && !args.create {
        bail!(
            "{} does not exist; pass --create to make a new collection",
            args.collection.display()
        );
    }
    // bind before opening, so a port clash leaves the collection untouched
    let listener = TcpListener::bind(SocketAddr::new(args.host, args.port))
        .await
        .with_context(|| format!("couldn't listen on {}:{}", args.host, args.port))?;
    let port = listener.local_addr()?.port();

    let backups = Backups::new(&args.collection)?;
    // before opening it can change anything, such as by upgrading it
    if existed {
        backups.take_before_opening(&args.collection)?;
    }

    let backend = rpc::new_backend()?;
    let media_dir = open_collection(&backend, &args.collection)?;
    if args.seed_demo && !existed {
        seed::seed_demo_content(&backend)?;
    }
    if let (Some(path), false) = (&args.seed_file, existed) {
        seed::seed_from_file(&backend, path)?;
    }

    if existed {
        backups.remember_state(&backend);
    } else {
        backups.take(&backend);
    }
    let backups = Arc::new(backups);
    let periodic = tokio::spawn(backups::run_periodically(
        backups.clone(),
        backend.clone(),
        Duration::from_secs(args.backup_interval_secs),
    ));

    // Local tools reach the collection through this, as it's locked to this
    // process. Only over a private Unix socket, never TCP; see docs/admin-cli.md.
    let admin = Arc::new(Admin::new(
        backend.clone(),
        backups.clone(),
        &args.collection,
        media_dir.clone(),
    ));
    let admin_socket = AdminSocket::start(admin).await?;

    let hostname = machine_hostname();
    let state = Arc::new(AppState {
        backend: backend.clone(),
        web_root,
        media_dir,
        hosts: guard::AllowedHosts::new(port, hostname.as_deref(), &args.allow_hosts),
        frame_token: frame::new_token(),
    });
    let app = Router::new()
        .fallback(handle)
        .layer(middleware::from_fn_with_state(
            state.clone(),
            guard::check_request,
        ))
        .with_state(state);

    // Printed only once requests can succeed. The tests read this line.
    println!("Listening on http://{}:{port}", loopback_or(args.host));
    print_network_urls(args.host, hostname.as_deref(), port);

    let served = serve_until_signal(listener, app, backend.clone()).await;
    periodic.abort();
    admin_socket.stop();
    backups::take_if_changed_blocking(backups, backend.clone()).await;
    let closed = close_blocking(backend).await;
    served?;
    closed
}

/// The address that this machine can use to reach what it's listening on.
fn loopback_or(bound: IpAddr) -> String {
    match bound {
        IpAddr::V4(ip) if ip.is_unspecified() => "127.0.0.1".into(),
        IpAddr::V6(ip) if ip.is_unspecified() => "[::1]".into(),
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    }
}

fn print_network_urls(bound: IpAddr, hostname: Option<&str>, port: u16) {
    if bound.is_loopback() {
        return;
    }
    if !bound.is_unspecified() {
        print_no_login_warning();
        return;
    }
    if let Some(ip) = primary_lan_ip() {
        println!("On your network: http://{ip}:{port}");
    }
    if let Some(hostname) = hostname {
        let short = hostname.strip_suffix(".local").unwrap_or(hostname);
        println!("On your network: http://{short}.local:{port}");
    }
    print_no_login_warning();
}

fn print_no_login_warning() {
    println!("WARNING: there is no login, so anyone on your network can read and change this collection.");
}

/// The address this machine would use to reach the wider network. Connecting a
/// UDP socket only chooses a route, and sends nothing.
fn primary_lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("10.255.255.255:9").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

fn machine_hostname() -> Option<String> {
    #[cfg(unix)]
    {
        let uname = rustix::system::uname();
        let name = uname.nodename().to_str().ok()?;
        (!name.is_empty()).then(|| name.to_string())
    }
    #[cfg(not(unix))]
    {
        std::env::var("COMPUTERNAME").ok()
    }
}

/// The binary is at out/rust/<profile>/evolanki-server, beside out/web.
fn default_web_root() -> Result<PathBuf> {
    let exe = std::env::current_exe()?.canonicalize()?;
    exe.ancestors()
        .nth(3)
        .map(|out| out.join("web"))
        .context("the binary isn't in a build tree; pass --web-root")
}

fn check_web_root(web_root: &Path) -> Result<()> {
    let index = web_root.join("sveltekit/index.html");
    if !index.exists() {
        bail!(
            "{} not found. Build the pages with `./ninja web`, or \
             pass --web-root.",
            index.display()
        );
    }
    Ok(())
}

/// Pages and the API share one entry point, as in mediasrv.py.
async fn handle(State(state): State<Arc<AppState>>, request: Request) -> Response {
    match *request.method() {
        Method::POST => post(state, request).await,
        Method::GET | Method::HEAD => files::serve(&state, request).await,
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

async fn post(state: Arc<AppState>, request: Request) -> Response {
    let Some(method) = request
        .uri()
        .path()
        .strip_prefix("/_anki/")
        .filter(|m| !m.contains('/'))
        .map(str::to_string)
    else {
        return (StatusCode::NOT_FOUND, "only /_anki/<method> accepts POST").into_response();
    };
    let body = match axum::body::to_bytes(request.into_body(), MAX_REQUEST_BYTES).await {
        Ok(body) => body,
        Err(err) => return (StatusCode::PAYLOAD_TOO_LARGE, err.to_string()).into_response(),
    };
    rpc_response(state, method, body).await
}

async fn rpc_response(state: Arc<AppState>, method: String, body: Bytes) -> Response {
    // Backend calls block, some for minutes (FSRS optimisation). The backend
    // serialises collection access with its own mutex, while calls that don't touch
    // the collection, such as latestProgress and setWantsAbort, go straight
    // through - the progress UI depends on that.
    let name = method.clone();
    let result =
        tokio::task::spawn_blocking(move || rpc::run(&state.backend, &method, &body)).await;
    match result {
        Ok(Ok(output)) if output.is_empty() => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(output)) => ([(header::CONTENT_TYPE, "application/binary")], output).into_response(),
        Ok(Err(err)) => {
            tracing::info!("{name}: {}", err.message);
            (err.status, err.message).into_response()
        }
        Err(err) => {
            tracing::error!("{name} panicked: {err}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("{name} failed unexpectedly; please restart the server"),
            )
                .into_response()
        }
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::signal;
        use tokio::signal::unix::SignalKind;
        let mut term = signal(SignalKind::terminate()).expect("signal handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = term.recv() => {},
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await.ok();
}

async fn serve_until_signal(listener: TcpListener, app: Router, backend: Backend) -> Result<()> {
    let (stopping_tx, mut stopping_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        println!("Shutting down");
        // lets a long-running operation stop at its next progress check
        let _ = rpc::call(&backend, "setWantsAbort", &[]);
        let _ = stopping_tx.send(true);
    });

    let mut graceful_rx = stopping_rx.clone();
    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        let _ = graceful_rx.changed().await;
    });
    let grace_expired = async move {
        let _ = stopping_rx.changed().await;
        tokio::time::sleep(SHUTDOWN_GRACE).await;
    };
    tokio::select! {
        result = server.into_future() => result.context("server failed"),
        _ = grace_expired => {
            tracing::warn!("connections still open after grace period");
            Ok(())
        }
    }
}

async fn close_blocking(backend: Backend) -> Result<()> {
    // waits for any running operation to release the collection
    tokio::task::spawn_blocking(move || close_collection(&backend))
        .await
        .context("closing the collection panicked")??;
    println!("Collection closed");
    Ok(())
}
