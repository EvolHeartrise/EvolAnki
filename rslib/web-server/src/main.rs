// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! A local, single-user web server around Anki's Rust backend. See
//! docs/web-server.md.

mod files;
mod guard;
mod rpc;
mod seed;

use std::future::IntoFuture;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anki::backend::Backend;
use anki_proto::collection::CloseCollectionRequest;
use anki_proto::collection::OpenCollectionRequest;
use anki_proto::generic;
use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use axum::body::Bytes;
use axum::extract::DefaultBodyLimit;
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
use tokio::net::TcpListener;
use tokio::sync::watch;

use crate::rpc::call_typed;

/// Enough for any page's request; media uploads will want a dedicated route.
const MAX_REQUEST_BYTES: usize = 64 * 1024 * 1024;
/// How long open connections get to finish once shutdown starts.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

#[derive(Parser)]
#[command(about = "Serve an Anki collection to a browser on this machine")]
struct Args {
    /// Path to the collection (.anki2). Media lives in the matching .media
    /// folder, as in a desktop profile.
    #[arg(long)]
    collection: PathBuf,
    /// Port on 127.0.0.1. 0 picks a free one.
    #[arg(long, default_value_t = 8765)]
    port: u16,
    /// Address to bind to. Exists only so that asking for anything but
    /// 127.0.0.1 gets a clear refusal: there is no login.
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
    /// Folder of built web assets, containing sveltekit/index.html.
    #[arg(long, default_value = "out/qt/_aqt/data/web")]
    web_root: PathBuf,
    /// Create the collection if it doesn't exist. Without this, a mistyped
    /// path is an error rather than a new empty collection.
    #[arg(long)]
    create: bool,
    /// When creating a collection, add sample notes and reviews to it.
    #[arg(long, requires = "create")]
    seed_demo: bool,
    /// Language for backend messages and page text.
    #[arg(long, default_value = "en")]
    lang: String,
}

pub struct AppState {
    backend: Backend,
    web_root: PathBuf,
    media_dir: PathBuf,
    /// The port actually bound, which the Host check compares against.
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    anki::log::set_global_logger(None)?;
    run(Args::parse()).await
}

async fn run(args: Args) -> Result<()> {
    if args.host != "127.0.0.1" {
        bail!(
            "refusing to bind to {}: there is no login, so only 127.0.0.1 is allowed",
            args.host
        );
    }
    check_web_root(&args.web_root)?;

    let existed = args.collection.exists();
    if !existed && !args.create {
        bail!(
            "{} does not exist; pass --create to make a new collection",
            args.collection.display()
        );
    }
    // bind before opening, so a port clash leaves the collection untouched
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, args.port)))
        .await
        .with_context(|| format!("couldn't listen on 127.0.0.1:{}", args.port))?;
    let port = listener.local_addr()?.port();

    let backend = rpc::new_backend(&args.lang)?;
    let media_dir = open_collection(&backend, &args.collection)?;
    if args.seed_demo && !existed {
        seed::seed_demo_content(&backend)?;
    }

    let state = Arc::new(AppState {
        backend: backend.clone(),
        web_root: args.web_root,
        media_dir,
        port,
    });
    let app = Router::new()
        .fallback(handle)
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            guard::check_request,
        ))
        .with_state(state);

    // Printed only once requests can succeed. The tests read this line.
    println!("Listening on http://127.0.0.1:{port}");

    let served = serve_until_signal(listener, app, backend.clone()).await;
    let closed = close_collection(backend).await;
    served?;
    closed
}

fn check_web_root(web_root: &Path) -> Result<()> {
    let index = web_root.join("sveltekit/index.html");
    if !index.exists() {
        bail!(
            "{} not found. Build the pages with `./ninja qt:aqt:data:web:sveltekit`, or \
             pass --web-root.",
            index.display()
        );
    }
    Ok(())
}

/// Returns the media folder.
fn open_collection(backend: &Backend, path: &Path) -> Result<PathBuf> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let parent = parent.unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent).with_context(|| parent.display().to_string())?;
    let path = parent.canonicalize()?.join(
        path.file_name()
            .context("collection path has no filename")?,
    );

    // same layout as media_paths_from_col_path() in pylib, so that collections are
    // interchangeable with a desktop profile
    let media_dir = path.with_extension("media");
    let media_db = path.with_extension("media.db2");
    std::fs::create_dir_all(&media_dir)?;

    let utf8 = |p: &Path| {
        p.to_str()
            .map(str::to_string)
            .with_context(|| format!("path is not UTF-8: {}", p.display()))
    };
    let _: generic::Empty = call_typed(
        backend,
        "openCollection",
        &OpenCollectionRequest {
            collection_path: utf8(&path)?,
            media_folder_path: utf8(&media_dir)?,
            media_db_path: utf8(&media_db)?,
        },
    )?;
    Ok(media_dir)
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

async fn close_collection(backend: Backend) -> Result<()> {
    // waits for any running operation to release the collection
    tokio::task::spawn_blocking(move || {
        call_typed::<generic::Empty>(
            &backend,
            "closeCollection",
            &CloseCollectionRequest {
                downgrade_to_schema11: false,
            },
        )
    })
    .await
    .context("closing the collection panicked")??;
    println!("Collection closed");
    Ok(())
}
