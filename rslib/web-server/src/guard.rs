// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Request checks. There's no login, and anyone on the network can talk to this
//! server. What these checks stop is other web sites using a visitor's browser
//! to do that:
//!
//! - The Host check defeats DNS rebinding, where evil.example resolves to one
//!   of our addresses and so counts as same-origin in the browser. A name that
//!   isn't ours, or an IP address that isn't private, can't be someone visiting
//!   us.
//! - The Origin, Sec-Fetch-Site and Content-Type checks stop other sites from
//!   driving the API. application/binary is not a CORS-safelisted type, so a
//!   cross-origin fetch would need a preflight, and we never grant one.
//!   Browsers leave Sec-Fetch-Site out over plain HTTP on a LAN address, as it
//!   isn't "potentially trustworthy", but they always send Origin with a POST.
//!
//! Card frames (frame.rs) are sandboxed, so their origin is "null" and these
//! checks refuse them. The one exception is reading from the frame's own
//! unguessable path prefix.

use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
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
    let null_origin_allowed = matches!(*request.method(), Method::GET | Method::HEAD)
        && request
            .uri()
            .path()
            .strip_prefix("/_frame/")
            .and_then(|path| path.strip_prefix(state.frame_token.as_str()))
            .is_some_and(|path| path.starts_with('/'));
    let verdict = check(
        request.method(),
        authority.as_deref(),
        request.headers(),
        &state.hosts,
        null_origin_allowed,
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
    headers
        .entry("cross-origin-resource-policy")
        .or_insert(HeaderValue::from_static("same-origin"));
    response
}

/// What a request's Host may be.
pub struct AllowedHosts {
    /// The port actually bound.
    pub port: u16,
    /// Lowercase names besides localhost: this machine's, and --allow-host.
    pub names: Vec<String>,
}

impl AllowedHosts {
    pub fn new(port: u16, hostname: Option<&str>, extra: &[String]) -> Self {
        let mut names: Vec<String> = extra.iter().map(|n| n.to_ascii_lowercase()).collect();
        if let Some(hostname) = hostname {
            let hostname = hostname.to_ascii_lowercase();
            // a machine may report either form
            let short = hostname.strip_suffix(".local").unwrap_or(&hostname);
            names.push(short.to_string());
            names.push(format!("{short}.local"));
        }
        Self { port, names }
    }

    /// `host` is a Host header's value, in lowercase.
    fn allows(&self, host: &str) -> bool {
        let Some((name, port)) = split_host_port(host) else {
            return false;
        };
        if port != self.port {
            return false;
        }
        name == "localhost"
            || self.names.iter().any(|allowed| allowed == name)
            || parse_ip_literal(name).is_some_and(ip_is_local)
    }
}

/// The name and port of a Host value, which must have a port. A browser leaves
/// it out for port 80, which isn't a port we expect to run on without root.
fn split_host_port(host: &str) -> Option<(&str, u16)> {
    let (name, port) = host.rsplit_once(':')?;
    // refuses "+80" and the like, which u16's parser would take
    if !port.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((name, port.parse().ok()?))
}

fn parse_ip_literal(name: &str) -> Option<IpAddr> {
    if let Some(v6) = name.strip_prefix('[').and_then(|n| n.strip_suffix(']')) {
        return v6.parse::<Ipv6Addr>().ok().map(IpAddr::V6);
    }
    // std accepts only plain dotted decimal, so not 0x7f.1 or 2130706433
    name.parse::<Ipv4Addr>().ok().map(IpAddr::V4)
}

/// Loopback, private and link-local. IPv4-mapped IPv6 and the like aren't, as
/// no browser needs them.
fn ip_is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => {
            let first = ip.segments()[0];
            ip.is_loopback() || (first & 0xfe00) == 0xfc00 || (first & 0xffc0) == 0xfe80
        }
    }
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
    hosts: &AllowedHosts,
    null_origin_allowed: bool,
) -> Result<(), String> {
    let host = single_header(headers, header::HOST)?
        .ok_or("missing host header")?
        .to_ascii_lowercase();
    if !hosts.allows(&host) {
        return Err(format!("host not allowed: {host}"));
    }
    // an absolute request target takes precedence over Host in some clients
    if let Some(authority) = uri_authority {
        if !authority.eq_ignore_ascii_case(&host) {
            return Err(format!("host not allowed: {authority}"));
        }
    }

    // each name for this server is a different origin, so one can't call another
    if let Some(origin) = single_header(headers, header::ORIGIN)? {
        let frame_read = null_origin_allowed && origin == "null";
        if !frame_read && !origin.eq_ignore_ascii_case(&format!("http://{host}")) {
            return Err(format!("origin not allowed: {origin}"));
        }
    }

    if method == Method::POST {
        if let Some(site) = single_header(headers, "sec-fetch-site".try_into().unwrap())? {
            if site != "same-origin" {
                return Err(format!("cross-site request refused: {site}"));
            }
        }
        // a cross-site form can't send this type without a CORS preflight
        if single_header(headers, header::CONTENT_TYPE)? != Some("application/binary") {
            return Err("content type must be application/binary".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;

    fn hosts() -> AllowedHosts {
        AllowedHosts::new(8765, Some("MyBox"), &["Tablet.Tail.Example".into()])
    }

    fn run(method: Method, list: &[(&'static str, &'static str)]) -> Result<(), String> {
        let mut headers = HeaderMap::new();
        for (name, value) in list {
            headers.append(*name, HeaderValue::from_static(value));
        }
        check(&method, None, &headers, &hosts(), false)
    }

    const BIN: (&str, &str) = ("content-type", "application/binary");

    #[test]
    fn allowed_hosts() {
        for host in [
            "127.0.0.1:8765",
            "127.1.2.3:8765",
            "LOCALHOST:8765",
            "localhost:8765",
            "[::1]:8765",
            "10.0.0.1:8765",
            "172.16.0.1:8765",
            "172.31.255.254:8765",
            "192.168.1.20:8765",
            "169.254.1.1:8765",
            "[fd12:3456::1]:8765",
            "[fe80::1]:8765",
            "mybox:8765",
            "MyBox.local:8765",
            "tablet.tail.example:8765",
        ] {
            assert!(run(Method::GET, &[("host", host)]).is_ok(), "{host}");
        }
    }

    #[test]
    fn refused_hosts() {
        for host in [
            // other names
            "evil.example:8765",
            "localhost.evil.example:8765",
            "127.0.0.1.evil.example:8765",
            "192.168.1.20.evil.example:8765",
            "mybox.evil.example:8765",
            "evil.mybox.local:8765",
            "other.local:8765",
            "local:8765",
            "mybox.local.:8765",
            "tail.example:8765",
            // public addresses
            "8.8.8.8:8765",
            "172.32.0.1:8765",
            "192.169.1.1:8765",
            "100.64.0.1:8765",
            "[2001:db8::1]:8765",
            "[::ffff:192.168.1.1]:8765",
            "0.0.0.0:8765",
            "[::]:8765",
            // spellings that only some parsers call private
            "0x7f.1:8765",
            "2130706433:8765",
            "010.0.0.1:8765",
            "192.168.1:8765",
            // wrong or missing ports
            "127.0.0.1",
            "localhost",
            "mybox",
            "127.0.0.1:8766",
            "192.168.1.20:80",
            "192.168.1.20:+8765",
            "192.168.1.20:",
            "127.0.0.1:8765.evil.example",
            "[::1]",
            "::1:8765",
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
    fn a_machine_named_with_local_gets_both_forms() {
        let hosts = AllowedHosts::new(1, Some("box.local"), &[]);
        assert!(hosts.allows("box:1") && hosts.allows("box.local:1"));
        assert!(!hosts.allows("box.local.local:1"));
    }

    #[test]
    fn origins_must_match_the_host_in_use() {
        let host = ("host", "192.168.1.20:8765");
        assert!(run(Method::POST, &[host, BIN]).is_ok());
        assert!(run(
            Method::POST,
            &[host, BIN, ("origin", "http://192.168.1.20:8765")]
        )
        .is_ok());
        for origin in [
            "http://127.0.0.1:8765",
            "http://192.168.1.21:8765",
            "http://evil.example",
            "null",
        ] {
            assert!(
                run(Method::POST, &[host, BIN, ("origin", origin)]).is_err(),
                "{origin}"
            );
        }
    }

    #[test]
    fn absolute_targets() {
        let mut headers = HeaderMap::new();
        headers.append("host", HeaderValue::from_static("127.0.0.1:8765"));
        assert!(check(
            &Method::GET,
            Some("127.0.0.1:8765"),
            &headers,
            &hosts(),
            false
        )
        .is_ok());
        assert!(check(
            &Method::GET,
            Some("evil.example"),
            &headers,
            &hosts(),
            false
        )
        .is_err());
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
    fn null_origin_is_only_for_frame_reads() {
        let mut headers = HeaderMap::new();
        headers.append("host", HeaderValue::from_static("127.0.0.1:8765"));
        headers.append("origin", HeaderValue::from_static("null"));
        assert!(check(&Method::GET, None, &headers, &hosts(), true).is_ok());
        assert!(check(&Method::GET, None, &headers, &hosts(), false).is_err());

        headers.insert("origin", HeaderValue::from_static("https://evil.example"));
        assert!(check(&Method::GET, None, &headers, &hosts(), true).is_err());
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
