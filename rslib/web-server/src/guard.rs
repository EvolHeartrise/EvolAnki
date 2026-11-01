// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Request checks. There's no login, so being reachable only from pages this
//! server served is the entire access control:
//!
//! - The Host check defeats DNS rebinding, where evil.example resolves to
//!   127.0.0.1 and so counts as same-origin in the browser.
//! - The Origin, Sec-Fetch-Site and Content-Type checks stop other sites from
//!   driving the API. application/binary is not a CORS-safelisted type, so a
//!   cross-origin fetch would need a preflight, and we never grant one.

use std::sync::Arc;

use axum::extract::Request;
use axum::extract::State;
use axum::http::header;
use axum::http::HeaderMap;
use axum::http::HeaderValue;
use axum::http::Method;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;

use crate::AppState;

pub async fn check_request(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Response {
    let authority = request.uri().authority().map(|a| a.as_str().to_string());
    let verdict = check(
        request.method(),
        authority.as_deref(),
        request.headers(),
        state.port,
    );
    let mut response = match verdict {
        Ok(()) => next.run(request).await,
        Err(reason) => {
            tracing::warn!("denied {} {}: {reason}", request.method(), request.uri());
            (StatusCode::FORBIDDEN, reason).into_response()
        }
    };
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    // stops other sites embedding media or pages with no-cors requests
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    response
}

fn single_header(headers: &HeaderMap, name: header::HeaderName) -> Result<Option<&str>, String> {
    let mut values = headers.get_all(&name).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(format!("multiple {name} headers"));
    }
    value
        .to_str()
        .map(Some)
        .map_err(|_| format!("invalid {name} header"))
}

fn check(
    method: &Method,
    uri_authority: Option<&str>,
    headers: &HeaderMap,
    port: u16,
) -> Result<(), String> {
    let host = single_header(headers, header::HOST)?
        .ok_or("missing host header")?
        .to_ascii_lowercase();
    if host != format!("127.0.0.1:{port}") && host != format!("localhost:{port}") {
        return Err(format!("host not allowed: {host}"));
    }
    // an absolute request target takes precedence over Host in some clients
    if let Some(authority) = uri_authority {
        if !authority.eq_ignore_ascii_case(&host) {
            return Err(format!("host not allowed: {authority}"));
        }
    }

    // localhost and 127.0.0.1 are different origins, so one can't call the other
    if let Some(origin) = single_header(headers, header::ORIGIN)? {
        if !origin.eq_ignore_ascii_case(&format!("http://{host}")) {
            return Err(format!("origin not allowed: {origin}"));
        }
    }

    if method == Method::POST {
        if let Some(site) = single_header(headers, "sec-fetch-site".try_into().unwrap())? {
            if site != "same-origin" {
                return Err(format!("cross-site request refused: {site}"));
            }
        }
        // as in mediasrv.py
        if single_header(headers, header::CONTENT_TYPE)? != Some("application/binary") {
            return Err("content type must be application/binary".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    fn run(method: Method, list: &[(&'static str, &'static str)]) -> Result<(), String> {
        let mut headers = HeaderMap::new();
        for (name, value) in list {
            headers.append(*name, HeaderValue::from_static(value));
        }
        check(&method, None, &headers, 8765)
    }

    const BIN: (&str, &str) = ("content-type", "application/binary");

    #[test]
    fn hosts() {
        assert!(run(Method::GET, &[("host", "127.0.0.1:8765")]).is_ok());
        assert!(run(Method::GET, &[("host", "LOCALHOST:8765")]).is_ok());
        for host in [
            "evil.example:8765",
            "127.0.0.1",
            "localhost",
            "127.0.0.1:8766",
            "localhost.evil.example:8765",
            "127.0.0.1:8765.evil.example",
            "[::1]:8765",
            "0.0.0.0:8765",
        ] {
            assert!(run(Method::GET, &[("host", host)]).is_err(), "{host}");
        }
        assert!(run(Method::GET, &[]).is_err());
        assert!(run(
            Method::GET,
            &[("host", "127.0.0.1:8765"), ("host", "evil.example")]
        )
        .is_err());
    }

    #[test]
    fn absolute_targets() {
        let mut headers = HeaderMap::new();
        headers.append("host", HeaderValue::from_static("127.0.0.1:8765"));
        assert!(check(&Method::GET, Some("127.0.0.1:8765"), &headers, 8765).is_ok());
        assert!(check(&Method::GET, Some("evil.example"), &headers, 8765).is_err());
    }

    #[test]
    fn origins() {
        let host = ("host", "127.0.0.1:8765");
        assert!(run(Method::POST, &[host, BIN]).is_ok());
        assert!(run(
            Method::POST,
            &[host, BIN, ("origin", "http://127.0.0.1:8765")]
        )
        .is_ok());
        for origin in [
            "https://evil.example",
            "null",
            "http://localhost:8765",
            "http://127.0.0.1:1",
            "https://127.0.0.1:8765",
            "http://127.0.0.1:8765.evil.example",
        ] {
            assert!(
                run(Method::POST, &[host, BIN, ("origin", origin)]).is_err(),
                "{origin}"
            );
            assert!(
                run(Method::GET, &[host, ("origin", origin)]).is_err(),
                "{origin}"
            );
        }
    }

    #[test]
    fn fetch_metadata_and_content_type() {
        let host = ("host", "127.0.0.1:8765");
        assert!(run(
            Method::POST,
            &[host, BIN, ("sec-fetch-site", "same-origin")]
        )
        .is_ok());
        assert!(run(Method::POST, &[host, BIN, ("sec-fetch-site", "cross-site")]).is_err());
        assert!(run(Method::POST, &[host, BIN, ("sec-fetch-site", "same-site")]).is_err());
        assert!(run(Method::POST, &[host]).is_err());
        assert!(run(Method::POST, &[host, ("content-type", "text/plain")]).is_err());
        // following a link from another site to a page is fine
        assert!(run(Method::GET, &[host, ("sec-fetch-site", "cross-site")]).is_ok());
    }
}
