// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! The sandboxed document that card HTML and deck descriptions run in, and the
//! few resources it may load. A sandboxed frame has an opaque origin, so to
//! this server it is a stranger: guard.rs refuses its API calls, and CORP
//! refuses it our usual responses. Everything it does need lives under
//! `/_frame/<token>/`, where the token is random per run, so that other web
//! sites, which get the same treatment as the frame, still can't embed the
//! user's media. Pages get the prefix from `/_anki/frameBase`.
//!
//! - `/_frame/<token>/m/`: the document. Its URL means that a card's relative
//!   media paths resolve to the next line.
//! - `/_frame/<token>/m/<file>`: collection media.
//! - `/_frame/<token>/a/<file>`: bundled assets on [is_frame_asset].

use axum::extract::Request;
use axum::http::header;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;

use crate::files;
use crate::AppState;

const DOCUMENT: &str = include_str!("card_frame.html");
const CARD_HEAD: &str = r#"<link rel="stylesheet" href="../a/css/reviewer.css">"#;
const CARD_STYLE: &str = "body { background-color: var(--canvas); }";
const CARD_BODY: &str = r#"<div id="qa" dir="auto"></div>
<script nonce="@NONCE@" src="../a/js/reviewer.js"></script>"#;
const STATIC_STYLE: &str = "body { margin: 0; padding: 0; color: var(--fg); \
     background: transparent; overflow-wrap: break-word; } \
     a { color: var(--fg-link); } img { max-width: 100%; }";
const STATIC_BODY: &str = r#"<div id="content"></div>"#;

pub fn new_token() -> String {
    format!("{:032x}", rand::random::<u128>())
}

/// The path prefix for this run.
pub fn base(state: &AppState) -> String {
    format!("/_frame/{}/", state.frame_token)
}

fn is_frame_asset(path: &str) -> bool {
    matches!(
        path,
        "css/webview.css" | "css/reviewer.css" | "js/reviewer.js" | "js/mathjax.js"
    ) || path.starts_with("js/vendor/mathjax/")
}

/// Handle a request for `/_frame/<rest>`.
pub async fn serve(state: &AppState, rest: &str, request: Request) -> Response {
    let path = rest
        .strip_prefix(state.frame_token.as_str())
        .and_then(|path| path.strip_prefix('/'));
    let mut response = match path {
        Some("m/") => {
            let host = files::request_host(&request);
            return document(state, &host, request.uri().query() == Some("static"));
        }
        Some(path) => {
            if let Some(file) = path.strip_prefix("m/") {
                files::media(state, file, request).await
            } else if let Some(file) = path.strip_prefix("a/").filter(|f| is_frame_asset(f)) {
                files::bundled(state, &files::request_host(&request), file, file, None).await
            } else {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
        // the same answer as a path that never existed
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let headers = response.headers_mut();
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("cross-origin"),
    );
    // fonts are always fetched with CORS, and a sandboxed page's origin is "null"
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("null"),
    );
    response
}

fn document(state: &AppState, host: &str, is_static: bool) -> Response {
    let nonce = new_token();
    let (head, style, body) = if is_static {
        ("", STATIC_STYLE, STATIC_BODY)
    } else {
        (CARD_HEAD, CARD_STYLE, CARD_BODY)
    };
    let html = DOCUMENT
        .replace("@HEAD@", head)
        .replace("@STYLE@", style)
        .replace("@BODY@", body)
        .replace("@NONCE@", &nonce);
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8".to_string()),
            (header::CACHE_CONTROL, "no-store".to_string()),
            (
                header::CONTENT_SECURITY_POLICY,
                document_csp(state, host, is_static, &nonce),
            ),
        ],
        html,
    )
        .into_response()
}

/// Only our nonced scripts and MathJax can run, and MathJax is limited to its
/// own files; card and media scripts have neither a nonce nor a matching path.
/// Nothing can connect anywhere, or load anything from outside the prefix, and
/// the sandbox directive holds even if the page is opened directly instead of
/// through the iframe that asks for it.
fn document_csp(state: &AppState, host: &str, is_static: bool, nonce: &str) -> String {
    let origin = format!("http://{host}");
    let ours = format!("{origin}/_frame/{}/", state.frame_token);
    let scripts = if is_static {
        format!("'nonce-{nonce}'")
    } else {
        format!("'nonce-{nonce}' {ours}a/js/mathjax.js {ours}a/js/vendor/mathjax/")
    };
    format!(
        "default-src 'none'; script-src {scripts}; style-src 'unsafe-inline' {ours}; \
         img-src data: blob: {ours}; media-src data: blob: {ours}; font-src data: {ours}; \
         connect-src 'none'; object-src 'none'; frame-src 'none'; child-src 'none'; \
         worker-src 'none'; manifest-src 'none'; base-uri 'none'; form-action 'none'; \
         frame-ancestors {origin}; sandbox allow-scripts"
    )
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn only_listed_assets_are_exposed() {
        assert!(is_frame_asset("js/reviewer.js"));
        assert!(is_frame_asset("js/vendor/mathjax/tex-chtml-full.js"));
        for path in ["sveltekit/index.html", "js/vendor/jquery.min.js", ""] {
            assert!(!is_frame_asset(path), "{path}");
        }
    }
}
