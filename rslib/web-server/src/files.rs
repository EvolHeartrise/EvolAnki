// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Static files: the built pages and assets, and collection media. Mirrors the
//! file handling and security headers of qt/aqt/mediasrv.py.

use std::io::ErrorKind;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;
use std::sync::LazyLock;

use axum::body::Body;
use axum::extract::Request;
use axum::http::header;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use percent_encoding::percent_decode_str;
use regex::bytes::Regex;
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::AppState;

/// Collection media is untrusted: an SVG or HTML file must not run as an active
/// document on our origin, where it could call the API. Same as
/// UNTRUSTED_MEDIA_CSP in mediasrv.py, which explains each directive.
const UNTRUSTED_MEDIA_CSP: &str = "default-src 'none'; script-src 'none'; connect-src 'none'; \
     object-src 'none'; frame-src 'none'; child-src 'none'; base-uri 'none'; form-action 'none'; \
     style-src 'self' 'unsafe-inline'; img-src 'self'; font-src 'self'; media-src 'self'; \
     sandbox allow-same-origin";

/// Our pages are always shown top-level.
const TRUSTED_PAGE_CSP: &str = "frame-ancestors 'none'";

/// Top-level paths handled by the single-page app; see get_sveltekit_route().
const SVELTEKIT_ROUTES: &[&str] = &[
    "graphs",
    "congrats",
    "card-info",
    "change-notetype",
    "deck-options",
    "import-anki-package",
    "import-csv",
    "import-page",
    "image-occlusion",
    "preferences",
    "editor",
];

/// These routes show note content, so scripts are locked to our bundled paths.
const UNTRUSTED_SVELTEKIT_ROUTES: &[&str] = &["editor", "image-occlusion"];

static SVELTEKIT_CSP_META: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"<meta http-equiv="content-security-policy" content="script-src 'self' ('sha256-[A-Za-z0-9+/=]+')">"#,
    )
    .unwrap()
});

fn mime_for_path(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "css" => "text/css",
        "js" | "mjs" => "application/javascript",
        "html" | "htm" => "text/html",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "json" => "application/json",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "opus" | "oga" => "audio/ogg",
        "wav" => "audio/wav",
        "flac" => "audio/flac",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogg" => "audio/ogg",
        "ogv" => "video/ogg",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
}

fn text_response(status: StatusCode, text: impl Into<String>) -> Response {
    // plain text, so reflecting a path needs no escaping
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        text.into(),
    )
        .into_response()
}

fn invalid_path(status: StatusCode, path: &str) -> Response {
    text_response(status, format!("Invalid path: {path}"))
}

/// Resolve a decoded, relative request path under `root`. None if it could
/// leave the folder. The target needn't exist, but if it does, symlinks are
/// resolved before the check.
fn safe_join(root: &Path, relative: &str) -> Option<PathBuf> {
    // backslash and colon are separators/prefixes on Windows, and never appear in
    // a valid media filename
    if relative.is_empty() || relative.contains(['\\', '\0', ':']) {
        return None;
    }
    let path = Path::new(relative);
    if !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    let root = root.canonicalize().ok()?;
    let joined = root.join(path);
    match joined.canonicalize() {
        Ok(resolved) => resolved.starts_with(&root).then_some(resolved),
        Err(err) if err.kind() == ErrorKind::NotFound => {
            // also catches a dangling symlink, whose target doesn't matter as it
            // can't be opened
            Some(joined)
        }
        Err(_) => None,
    }
}

fn is_sveltekit_route(path: &str) -> Option<&'static str> {
    let first = path.split('/').next().unwrap_or_default();
    SVELTEKIT_ROUTES.iter().find(|r| **r == first).copied()
}

/// Handle a GET/HEAD request, in the same order as _extract_request().
pub async fn serve(state: &AppState, request: Request) -> Response {
    let raw_path = request.uri().path().to_string();
    let raw_path = raw_path.strip_prefix('/').unwrap_or(&raw_path);
    let Ok(path) = percent_decode_str(raw_path).decode_utf8() else {
        return text_response(StatusCode::BAD_REQUEST, "path is not valid UTF-8");
    };
    let path = path.into_owned();

    if path == "_anki/readyz" {
        // the collection is opened before the socket is, so we're always ready
        return StatusCode::OK.into_response();
    }
    if path == "favicon.ico" {
        return bundled(state, "imgs/favicon.ico", &path, None).await;
    }
    if path.is_empty() {
        return text_response(
            StatusCode::NOT_FOUND,
            "No home page yet. Try /deck-options/1, /graphs, /congrats or /card-info/<card id>.",
        );
    }
    if let Some(route) = is_sveltekit_route(&path) {
        return bundled(state, "sveltekit/index.html", &path, Some(route)).await;
    }
    if path.starts_with("_app/") {
        return bundled(state, &format!("sveltekit/{path}"), &path, None).await;
    }
    if let Some(rest) = path.strip_prefix("_anki/") {
        return bundled(state, rest, &path, None).await;
    }
    if path.starts_with("_addons/") {
        return text_response(StatusCode::NOT_FOUND, "add-ons are not supported");
    }
    media(state, &path, request).await
}

async fn bundled(
    state: &AppState,
    relative: &str,
    request_path: &str,
    route: Option<&str>,
) -> Response {
    let Some(full) = safe_join(&state.web_root, relative) else {
        return invalid_path(StatusCode::FORBIDDEN, request_path);
    };
    let mut data = match tokio::fs::read(&full).await {
        Ok(data) => data,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return invalid_path(StatusCode::NOT_FOUND, request_path)
        }
        // eg a directory
        Err(err) => {
            tracing::debug!("{}: {err}", full.display());
            return invalid_path(StatusCode::FORBIDDEN, request_path);
        }
    };
    let mime = mime_for_path(relative);

    let csp = if let Some(route) = route {
        let Some(captures) = SVELTEKIT_CSP_META.captures(&data) else {
            return text_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "CSP meta tag not found in SvelteKit index page",
            );
        };
        if UNTRUSTED_SVELTEKIT_ROUTES.contains(&route) {
            let hash = String::from_utf8_lossy(&captures[1]).into_owned();
            Some(untrusted_sveltekit_csp(state.port, &hash))
        } else {
            // the page's own CSP would stop a trusted page's inline scripts
            data = SVELTEKIT_CSP_META.replace(&data, &b""[..]).into_owned();
            Some(TRUSTED_PAGE_CSP.to_string())
        }
    } else if mime == "text/html" {
        Some(TRUSTED_PAGE_CSP.to_string())
    } else {
        None
    };

    let mut response = ([(header::CONTENT_TYPE, mime)], Body::from(data)).into_response();
    let headers = response.headers_mut();
    if let Some(csp) = csp {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_str(&csp).expect("CSP is ASCII"),
        );
    }
    if request_path.contains("/immutable/") {
        headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    response
}

/// Scripts may only come from our bundled paths, so a script in a media file or
/// note field can't run. Pages can be opened via either allowed host.
fn untrusted_sveltekit_csp(port: u16, script_hash: &str) -> String {
    let sources: Vec<String> = ["127.0.0.1", "localhost"]
        .iter()
        .flat_map(|host| {
            ["_anki", "_app"]
                .iter()
                .map(move |dir| format!("http://{host}:{port}/{dir}/"))
        })
        .collect();
    format!(
        "script-src {} {script_hash}; form-action 'none'; {TRUSTED_PAGE_CSP}",
        sources.join(" ")
    )
}

async fn media(state: &AppState, path: &str, request: Request) -> Response {
    let Some(full) = safe_join(&state.media_dir, path) else {
        return invalid_path(StatusCode::FORBIDDEN, path);
    };
    match tokio::fs::metadata(&full).await {
        Ok(meta) if meta.is_dir() => {
            return text_response(
                StatusCode::FORBIDDEN,
                format!("{path} is a directory (not supported)"),
            )
        }
        Ok(_) => (),
        Err(_) => return invalid_path(StatusCode::NOT_FOUND, path),
    }

    // ServeFile adds conditional and range requests, which audio/video seeking needs
    let mut response = match ServeFile::new(&full).oneshot(request).await {
        Ok(response) => response.into_response(),
        Err(err) => return text_response(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    };
    let max_age = match Path::new(path).extension().and_then(|e| e.to_str()) {
        Some("css") => "max-age=10",
        Some("js") => "max-age=0",
        _ => "max-age=3600",
    };
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(mime_for_path(path)),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(max_age));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(UNTRUSTED_MEDIA_CSP),
    );
    response
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn safe_join_refuses_escapes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("media");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(root.join("ok.png"), "x").unwrap();
        std::fs::write(dir.path().join("secret"), "x").unwrap();

        assert!(safe_join(&root, "ok.png").is_some());
        assert!(safe_join(&root, "missing.png").is_some());
        for bad in [
            "../secret",
            "a/../../secret",
            "a/../ok.png",
            "./ok.png",
            "/etc/passwd",
            "..",
            "",
            "..\\secret",
            "c:/windows",
            "x\0y",
        ] {
            assert!(safe_join(&root, bad).is_none(), "{bad:?}");
        }

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("secret"), root.join("link")).unwrap();
            assert!(safe_join(&root, "link").is_none());
        }
    }

    #[test]
    fn csp_meta_regex_matches_format_sveltekit_emits() {
        let html = br#"<meta http-equiv="content-security-policy" content="script-src 'self' 'sha256-wGlR/to4884xdAxqa0AdaCE3s8/gsByfJWsm6PjPBlU='">"#;
        let caps = SVELTEKIT_CSP_META.captures(html).unwrap();
        assert_eq!(
            &caps[1],
            b"'sha256-wGlR/to4884xdAxqa0AdaCE3s8/gsByfJWsm6PjPBlU='"
        );
    }
}
