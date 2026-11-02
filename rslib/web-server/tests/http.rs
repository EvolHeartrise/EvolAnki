// Copyright: Ankitects Pty Ltd and contributors
// License: GNU AGPL, version 3 or later; http://www.gnu.org/licenses/agpl.html

//! Runs the real binary against a throwaway collection and talks to it over a
//! raw socket, as a typical HTTP client would normalise away the paths and
//! headers these tests need to send.

#![cfg(unix)]

use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;
use std::path::Path;
use std::path::PathBuf;
use std::process::Child;
use std::process::Command;
use std::process::Stdio;

use anki_proto::collection::OpChanges;
use anki_proto::deck_config::DeckConfigsForUpdate;
use anki_proto::deck_config::UpdateDeckConfigsMode;
use anki_proto::deck_config::UpdateDeckConfigsRequest;
use anki_proto::decks::DeckId;
use anki_proto::generic;
use prost::Message;
use tempfile::TempDir;

const INDEX: &str = r#"<html><head><meta http-equiv="content-security-policy" content="script-src 'self' 'sha256-abc='"></head></html>"#;

/// Cargo builds the binary next to the deps/ folder this test runs from.
fn binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_evolanki-server")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let test_exe = std::env::current_exe().unwrap();
            test_exe.parent().unwrap().with_file_name("evolanki-server")
        })
}

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let web = dir.path().join("web");
        fs::create_dir_all(web.join("sveltekit/_app/immutable")).unwrap();
        fs::write(web.join("sveltekit/index.html"), INDEX).unwrap();
        fs::write(web.join("sveltekit/_app/immutable/a.mjs"), "1").unwrap();
        fs::write(dir.path().join("secret.txt"), "top secret").unwrap();
        Self { dir }
    }

    fn collection(&self) -> PathBuf {
        self.dir.path().join("col.anki2")
    }

    fn media(&self) -> PathBuf {
        self.dir.path().join("col.media")
    }

    fn command(&self) -> Command {
        let mut cmd = Command::new(binary());
        cmd.arg("--collection")
            .arg(self.collection())
            .arg("--web-root")
            .arg(self.dir.path().join("web"))
            .args(["--port", "0", "--host", "127.0.0.1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    fn start(&self) -> Server {
        self.start_with(&["--create"])
    }

    fn start_seeded(&self) -> Server {
        self.start_with(&["--create", "--seed-demo"])
    }

    fn start_with(&self, args: &[&str]) -> Server {
        let mut child = self.command().args(args).spawn().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let port = line
            .trim()
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .unwrap_or_else(|| {
                let mut err = String::new();
                child.stderr.take().unwrap().read_to_string(&mut err).ok();
                panic!("no port in {line:?}: {err}")
            });
        Server {
            child,
            stdout,
            port,
        }
    }
}

struct Server {
    child: Child,
    stdout: BufReader<std::process::ChildStdout>,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.child.kill().ok();
        self.child.wait().ok();
    }
}

struct Reply {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Reply {
    fn header(&self, name: &str) -> Option<String> {
        self.head.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_string())
        })
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

impl Server {
    /// Send a request with exactly these headers, plus a body length.
    fn raw(&self, method: &str, target: &str, headers: &[(&str, &str)], body: &[u8]) -> Reply {
        let mut request = format!("{method} {target} HTTP/1.1\r\nConnection: close\r\n");
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        request.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        stream.write_all(body).unwrap();
        let mut response = vec![];
        stream.read_to_end(&mut response).unwrap();
        let split = response
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("no end of headers");
        let head = String::from_utf8_lossy(&response[..split]).into_owned();
        let status = head.split(' ').nth(1).unwrap().parse().unwrap();
        Reply {
            status,
            head,
            body: response[split + 4..].to_vec(),
        }
    }

    fn host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    fn get(&self, target: &str) -> Reply {
        self.raw("GET", target, &[("Host", &self.host())], b"")
    }

    fn rpc_raw(&self, method: &str, input: &[u8]) -> Reply {
        self.raw(
            "POST",
            &format!("/_anki/{method}"),
            &[
                ("Host", &self.host()),
                ("Origin", &format!("http://{}", self.host())),
                ("Content-Type", "application/binary"),
                ("Anki-Op-Changes", "0"),
            ],
            input,
        )
    }

    fn rpc<O: Message + Default>(&self, method: &str, input: &impl Message) -> O {
        let reply = self.rpc_raw(method, &input.encode_to_vec());
        assert!(
            reply.status == 200 || reply.status == 204,
            "{method}: {} {}",
            reply.status,
            reply.text()
        );
        O::decode(reply.body.as_slice()).unwrap()
    }

    /// SIGTERM, then check that the collection was closed in an orderly
    /// fashion.
    fn stop(mut self) {
        let pid = self.child.id().to_string();
        assert!(Command::new("kill").arg(&pid).status().unwrap().success());
        let mut output = String::new();
        self.stdout.read_to_string(&mut output).unwrap();
        assert!(output.contains("Collection closed"), "{output}");
        assert!(self.child.wait().unwrap().success());
    }
}

fn new_per_day(server: &Server) -> (DeckConfigsForUpdate, u32) {
    let info: DeckConfigsForUpdate = server.rpc("getDeckConfigsForUpdate", &DeckId { did: 1 });
    let value = info
        .config
        .as_ref()
        .unwrap()
        .config
        .as_ref()
        .unwrap()
        .new_per_day;
    (info, value)
}

#[test]
fn deck_options_save_persists_across_restart() {
    let fixture = Fixture::new();
    let server = fixture.start();
    let (info, before) = new_per_day(&server);
    let wanted = before + 17;

    let mut config = info.config.clone().unwrap();
    config.config.as_mut().unwrap().new_per_day = wanted;
    let _: OpChanges = server.rpc(
        "updateDeckConfigs",
        &UpdateDeckConfigsRequest {
            target_deck_id: 1,
            config: Some(config),
            mode: UpdateDeckConfigsMode::Normal as i32,
            limits: info.current_deck.as_ref().unwrap().limits,
            ..Default::default()
        },
    );
    assert_eq!(new_per_day(&server).1, wanted);
    server.stop();

    let server = fixture.start();
    assert_eq!(new_per_day(&server).1, wanted);
    server.stop();
}

#[test]
fn rpc_replies_are_binary_and_backend_errors_are_plain_text() {
    let fixture = Fixture::new();
    let server = fixture.start();

    // binary output
    let reply = server.rpc_raw("getGraphPreferences", b"");
    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.header("content-type").as_deref(),
        Some("application/binary")
    );

    // backend errors are a 500 with the message as plain text
    let missing_card = anki_proto::card_rendering::RenderExistingCardRequest {
        card_id: 123,
        ..Default::default()
    };
    let reply = server.rpc_raw("renderExistingCard", &missing_card.encode_to_vec());
    assert_eq!(reply.status, 500);
    assert!(!reply.text().is_empty());
    let reply = server.rpc_raw("getDeckConfigsForUpdate", b"\xff\xff\xff");
    assert_eq!(reply.status, 500);
}

#[test]
fn methods_off_the_allow_list_are_refused() {
    let fixture = Fixture::new();
    let server = fixture.start();

    for method in [
        "closeCollection",
        "checkDatabase",
        "syncLogin",
        "importAnkiPackage",
        "addMediaFromPath",
        "getAbsoluteMediaPath",
        // removed from the pages, so nothing on the network should reach them
        "suspendCards",
        "changeNotetype",
        "getChangeNotetypeInfo",
        "doesNotExist",
    ] {
        assert_eq!(server.rpc_raw(method, b"").status, 404, "{method}");
    }
    // still open
    let _: DeckConfigsForUpdate = server.rpc("getDeckConfigsForUpdate", &DeckId { did: 1 });

    // POST can't reach anything else
    let host = server.host();
    let headers = [
        ("Host", host.as_str()),
        ("Content-Type", "application/binary"),
    ];
    assert_eq!(server.raw("POST", "/graphs", &headers, b"").status, 404);
    assert_eq!(
        server
            .raw("POST", "/_anki/a/closeCollection", &headers, b"")
            .status,
        404
    );
    assert_eq!(server.raw("DELETE", "/graphs", &headers, b"").status, 405);
}

#[test]
fn bad_hosts_and_origins_are_refused() {
    let fixture = Fixture::new();
    let server = fixture.start();
    let host = server.host();
    let localhost = format!("localhost:{}", server.port);
    let binary = ("Content-Type", "application/binary");
    let path = "/_anki/getGraphPreferences";

    assert_eq!(
        server.raw("GET", "/graphs", &[("Host", &host)], b"").status,
        200
    );
    assert_eq!(
        server
            .raw("GET", "/graphs", &[("Host", &localhost)], b"")
            .status,
        200
    );
    assert_eq!(
        server
            .raw("POST", path, &[("Host", &localhost), binary], b"")
            .status,
        200
    );

    // DNS rebinding
    for bad in [
        "evil.example",
        "evil.example:80",
        "8.8.8.8",
        "127.0.0.1",
        "localhost:1",
    ] {
        assert_eq!(
            server.raw("GET", "/graphs", &[("Host", bad)], b"").status,
            403,
            "{bad}"
        );
        assert_eq!(
            server
                .raw("POST", path, &[("Host", bad), binary], b"")
                .status,
            403,
            "{bad}"
        );
    }
    assert_eq!(server.raw("GET", "/graphs", &[], b"").status, 403);
    let absolute = format!("http://evil.example:{}/graphs", server.port);
    assert_eq!(
        server.raw("GET", &absolute, &[("Host", &host)], b"").status,
        403
    );

    let bad_ip = format!("8.8.8.8:{}", server.port);
    assert_eq!(
        server
            .raw("GET", "/graphs", &[("Host", &bad_ip)], b"")
            .status,
        403
    );
    let named = format!("evil.example:{}", server.port);
    assert_eq!(
        server
            .raw("GET", "/graphs", &[("Host", &named)], b"")
            .status,
        403
    );

    // private addresses are fine, with an Origin to match
    let private = format!("192.168.7.7:{}", server.port);
    assert_eq!(
        server
            .raw("POST", path, &[("Host", &private), binary], b"")
            .status,
        200
    );
    let origin = format!("http://{private}");
    let headers = [
        ("Host", private.as_str()),
        ("Origin", origin.as_str()),
        binary,
    ];
    assert_eq!(server.raw("POST", path, &headers, b"").status, 200);
    let local_origin = format!("http://{host}");
    let headers = [
        ("Host", private.as_str()),
        ("Origin", local_origin.as_str()),
        binary,
    ];
    assert_eq!(server.raw("POST", path, &headers, b"").status, 403);

    // other sites
    for origin in ["https://evil.example", "null", "http://localhost:1"] {
        let headers = [("Host", host.as_str()), ("Origin", origin), binary];
        assert_eq!(
            server.raw("POST", path, &headers, b"").status,
            403,
            "{origin}"
        );
    }
    let headers = [
        ("Host", host.as_str()),
        ("Sec-Fetch-Site", "cross-site"),
        binary,
    ];
    assert_eq!(server.raw("POST", path, &headers, b"").status, 403);

    // types a cross-origin form or simple fetch can send
    let headers = [("Host", host.as_str()), ("Content-Type", "text/plain")];
    assert_eq!(server.raw("POST", path, &headers, b"").status, 403);
    assert_eq!(
        server.raw("POST", path, &[("Host", &host)], b"").status,
        403
    );
}

#[test]
fn media_is_served_safely() {
    let fixture = Fixture::new();
    let server = fixture.start();
    let media = fixture.media();
    fs::write(media.join("pic.svg"), "<svg/>").unwrap();
    fs::write(media.join("sp ace é.txt"), "0123456789").unwrap();
    fs::create_dir(media.join("folder")).unwrap();

    let reply = server.get("/pic.svg");
    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.header("content-type").as_deref(),
        Some("image/svg+xml")
    );
    let csp = reply.header("content-security-policy").unwrap();
    assert!(
        csp.contains("script-src 'none'") && csp.contains("sandbox"),
        "{csp}"
    );

    let reply = server.get("/sp%20ace%20%C3%A9.txt");
    assert_eq!(reply.text(), "0123456789");

    // ranges are needed for audio seeking
    let host = server.host();
    let range = [("Host", host.as_str()), ("Range", "bytes=2-4")];
    let reply = server.raw("GET", "/sp%20ace%20%C3%A9.txt", &range, b"");
    assert_eq!(reply.status, 206);
    assert_eq!(reply.text(), "234");

    assert_eq!(server.get("/missing.png").status, 404);
    assert_eq!(server.get("/folder").status, 403);
    assert_eq!(server.get("/not-there").status, 404);

    for bad in [
        "/../secret.txt",
        "/..%2fsecret.txt",
        "/%2e%2e/secret.txt",
        "/%2e%2e%2fsecret.txt",
        "/folder/../../secret.txt",
        "/folder/%2e%2e/%2e%2e/secret.txt",
        "/..%5csecret.txt",
        "//etc/passwd",
        "/%2fetc%2fpasswd",
        "/_anki/../../secret.txt",
        "/_anki/%2e%2e/%2e%2e/secret.txt",
        "/_app/%2e%2e/%2e%2e/%2e%2e/secret.txt",
        "/%00",
    ] {
        let reply = server.get(bad);
        assert_eq!(reply.status, 403, "{bad}: {}", reply.text());
        assert!(!reply.text().contains("top secret"), "{bad}");
    }

    // nor can a symlink inside the folder lead out of it
    std::os::unix::fs::symlink(fixture.dir.path().join("secret.txt"), media.join("l.txt")).unwrap();
    assert_eq!(server.get("/l.txt").status, 403);
}

#[test]
fn pages_have_the_expected_headers() {
    let fixture = Fixture::new();
    let server = fixture.start();

    for trusted in ["/deck-options/1", "/graphs"] {
        let reply = server.get(trusted);
        assert_eq!(reply.status, 200, "{trusted}");
        assert_eq!(reply.header("content-type").as_deref(), Some("text/html"));
        assert_eq!(
            reply.header("content-security-policy").as_deref(),
            Some("frame-ancestors 'none'")
        );
        assert!(
            !reply.text().contains("content-security-policy"),
            "{trusted}"
        );
        assert_eq!(
            reply.header("x-content-type-options").as_deref(),
            Some("nosniff")
        );
    }

    assert!(reply_is_web_app(&server, "/deck-options/1"));
    assert!(reply_is_web_app(&server, "/"));

    for framing in ["/", "/overview/1", "/review"] {
        let csp = server
            .get(framing)
            .header("content-security-policy")
            .unwrap();
        assert!(csp.contains("frame-src 'self'"), "{framing}: {csp}");
        assert!(csp.contains("script-src http://127.0.0.1:"), "{csp}");
        assert!(!csp.contains("unsafe-inline"), "{csp}");
    }

    // and they name whichever host was used
    let private = format!("192.168.7.7:{}", server.port);
    let reply = server.raw("GET", "/review", &[("Host", &private)], b"");
    assert_eq!(reply.status, 200);
    let csp = reply.header("content-security-policy").unwrap();
    assert!(
        csp.contains(&format!("script-src http://{private}/_anki/")),
        "{csp}"
    );
    assert!(!csp.contains("127.0.0.1"), "{csp}");

    let reply = server.get("/_app/immutable/a.mjs");
    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.header("content-type").as_deref(),
        Some("application/javascript")
    );
    assert_eq!(
        reply.header("cache-control").as_deref(),
        Some("max-age=31536000")
    );
    assert_eq!(server.get("/_anki/sveltekit/index.html").status, 200);
    assert_eq!(server.get("/_app/immutable/missing.mjs").status, 404);
    assert_eq!(server.get("/_anki/readyz").status, 200);
    assert_eq!(server.get("/_addons/x/y").status, 404);
}

fn reply_is_web_app(server: &Server, path: &str) -> bool {
    server.get(path).text().contains("/_anki/theme.js")
}

/// The API must stay shut to a sandboxed frame, whose origin is "null", while
/// the frame's own prefix serves media and nothing else.
#[test]
fn frame_routes_are_narrow() {
    let fixture = Fixture::new();
    let server = fixture.start();
    fs::write(fixture.media().join("pic.png"), "png").unwrap();
    fs::write(fixture.dir.path().join("web/secret.js"), "1").unwrap();
    fs::create_dir_all(fixture.dir.path().join("web/js")).unwrap();
    fs::write(fixture.dir.path().join("web/js/reviewer.js"), "1").unwrap();
    fs::write(fixture.dir.path().join("web/js/editor.js"), "1").unwrap();

    let host = server.host();
    let base = server.get("/_anki/frameBase").text();
    assert!(
        base.starts_with("/_frame/") && base.ends_with('/'),
        "{base}"
    );
    let null_origin = [("Host", host.as_str()), ("Origin", "null")];

    // a sandboxed frame or other site can't learn the prefix
    let reply = server.raw("GET", "/_anki/frameBase", &null_origin, b"");
    assert_eq!(reply.status, 403);
    let cross = [("Host", host.as_str()), ("Sec-Fetch-Site", "cross-site")];
    assert_eq!(
        server.raw("GET", "/_anki/frameBase", &cross, b"").status,
        403
    );

    // embeddable through the prefix only
    let reply = server.raw("GET", &format!("{base}m/pic.png"), &null_origin, b"");
    assert_eq!(reply.status, 200);
    assert_eq!(reply.text(), "png");
    assert_eq!(
        reply.header("cross-origin-resource-policy").as_deref(),
        Some("cross-origin")
    );
    assert_eq!(
        reply.header("access-control-allow-origin").as_deref(),
        Some("null")
    );
    assert!(reply
        .header("content-security-policy")
        .unwrap()
        .contains("script-src 'none'"));

    let reply = server.get("/pic.png");
    assert_eq!(
        reply.header("cross-origin-resource-policy").as_deref(),
        Some("same-origin")
    );
    assert_eq!(reply.header("access-control-allow-origin"), None);
    assert_eq!(server.raw("GET", "/pic.png", &null_origin, b"").status, 403);

    // a wrong token is just a missing path, and null origin is refused before that
    assert_eq!(server.get("/_frame/nope/m/pic.png").status, 404);
    let reply = server.raw("GET", "/_frame/nope/m/pic.png", &null_origin, b"");
    assert_eq!(reply.status, 403);

    // listed assets only
    assert_eq!(server.get(&format!("{base}a/js/reviewer.js")).status, 200);
    assert_eq!(server.get(&format!("{base}a/js/editor.js")).status, 404);
    assert_eq!(server.get(&format!("{base}a/secret.js")).status, 404);
    assert_eq!(server.get(&format!("{base}other")).status, 404);
    for bad in [
        "m/../../../secret.txt",
        "m/%2e%2e/%2e%2e/secret.txt",
        "a/js/../secret.js",
    ] {
        assert_ne!(server.get(&format!("{base}{bad}")).status, 200, "{bad}");
    }

    // the API is unchanged, even for a caller who has the token
    let binary = [
        ("Host", host.as_str()),
        ("Origin", "null"),
        ("Content-Type", "application/binary"),
    ];
    for path in [
        "/_anki/getQueuedCards".to_string(),
        format!("{base}m/x"),
        format!("{base}../../_anki/getQueuedCards"),
    ] {
        assert_eq!(
            server.raw("POST", &path, &binary, b"").status,
            403,
            "{path}"
        );
    }
    let reply = server.raw("GET", "/_anki/js/reviewer.js", &null_origin, b"");
    assert_eq!(reply.status, 403);

    // the document, with a CSP of its own that keeps its sandbox if opened directly
    let reply = server.get(&format!("{base}m/"));
    assert_eq!(reply.status, 200);
    let csp = reply.header("content-security-policy").unwrap();
    for wanted in [
        "sandbox allow-scripts",
        "connect-src 'none'",
        "form-action 'none'",
        "base-uri 'none'",
        "frame-src 'none'",
        "'unsafe-inline'",
    ] {
        assert!(csp.contains(wanted), "{wanted}: {csp}");
    }
    assert!(!csp.contains("allow-same-origin"), "{csp}");
    assert!(reply.text().contains(r#"id="qa""#));

    let reply = server.get(&format!("{base}m/?static"));
    let csp = reply.header("content-security-policy").unwrap();
    let script_src = csp.split(';').find(|d| d.contains("script-src")).unwrap();
    assert!(script_src.contains("'nonce-"), "{csp}");
    assert!(!script_src.contains("unsafe"), "{csp}");
    assert!(reply.text().contains(r#"id="content""#));
}

fn fails_with(fixture: &Fixture, args: &[&str], message: &str) {
    let output = fixture.command().args(args).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(message), "{stderr}");
}

#[test]
fn refuses_unsafe_startup() {
    let fixture = Fixture::new();
    let output = Command::new(binary())
        .arg("--collection")
        .arg(fixture.collection())
        .args(["--create", "--host", "localhost"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid value"));
    assert!(!fixture.collection().exists());

    fails_with(&fixture, &[], "pass --create");
    assert!(!fixture.collection().exists());

    let output = Command::new(binary())
        .arg("--collection")
        .arg(fixture.collection())
        .args([
            "--create",
            "--port",
            "0",
            "--host",
            "127.0.0.1",
            "--web-root",
            "/nonexistent",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not found"));
    assert!(!fixture.collection().exists());
}

#[test]
fn listens_on_the_network_by_default_and_says_so() {
    let fixture = Fixture::new();
    let mut child = Command::new(binary())
        .arg("--collection")
        .arg(fixture.collection())
        .arg("--web-root")
        .arg(fixture.dir.path().join("web"))
        .args(["--port", "0", "--create", "--allow-host", "Phone.Example"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut lines = vec![];
    loop {
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let done = line.contains("WARNING") || line.is_empty();
        lines.push(line);
        if done {
            break;
        }
    }
    let port = lines[0].trim().rsplit(':').next().unwrap().to_string();
    assert!(
        lines[0].starts_with("Listening on http://127.0.0.1:"),
        "{lines:?}"
    );
    assert!(lines.last().unwrap().contains("no login"), "{lines:?}");
    let server = Server {
        child,
        stdout,
        port: port.parse().unwrap(),
    };
    // --allow-host, and its case doesn't matter
    let named = format!("phone.example:{port}");
    assert_eq!(
        server
            .raw("GET", "/graphs", &[("Host", &named)], b"")
            .status,
        200
    );
    let other = format!("tablet.example:{port}");
    assert_eq!(
        server
            .raw("GET", "/graphs", &[("Host", &other)], b"")
            .status,
        403
    );
    server.stop();
}

fn backup_files(fixture: &Fixture) -> Vec<PathBuf> {
    let folder = fixture.dir.path().join("backups");
    let mut files: Vec<PathBuf> = fs::read_dir(&folder)
        .map(|entries| entries.map(|e| e.unwrap().path()).collect())
        .unwrap_or_default();
    // not by name, as two within a second have a suffix, which sorts first
    files.sort_by_key(|file| fs::metadata(file).unwrap().modified().unwrap());
    files
}

/// (notes, cards, reviews, a sum over what reviews change)
fn contents(path: &Path) -> (i64, i64, i64, i64) {
    let db =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    db.query_row(
        "select (select count() from notes), (select count() from cards),
            (select count() from revlog),
            (select sum(due + ivl + reps + lapses + queue) from cards)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )
    .unwrap()
}

fn quick_check(path: &Path) {
    anki::storage::verify_copied_collection(path).unwrap();
}

fn answer_next(server: &Server) -> bool {
    use anki_proto::scheduler::card_answer::Rating;
    use anki_proto::scheduler::CardAnswer;
    use anki_proto::scheduler::GetQueuedCardsRequest;
    use anki_proto::scheduler::QueuedCards;

    let queued: QueuedCards = server.rpc(
        "getQueuedCards",
        &GetQueuedCardsRequest {
            fetch_limit: 1,
            intraday_learning_only: false,
        },
    );
    let Some(queued) = queued.cards.into_iter().next() else {
        return false;
    };
    let states = queued.states.unwrap();
    let _: OpChanges = server.rpc(
        "answerCard",
        &CardAnswer {
            card_id: queued.card.unwrap().id,
            current_state: states.current,
            new_state: states.easy,
            rating: Rating::Easy as i32,
            answered_at_millis: 1_700_000_000_000,
            milliseconds_taken: 1000,
        },
    );
    true
}

#[test]
fn backups_at_start_and_shutdown_open_and_match() {
    let fixture = Fixture::new();
    let server = fixture.start_seeded();
    assert_eq!(backup_files(&fixture).len(), 1, "one at start");
    assert!(answer_next(&server));
    server.stop();

    let files = backup_files(&fixture);
    assert_eq!(files.len(), 2, "one at shutdown, as a review happened");
    quick_check(&files[1]);
    assert_eq!(contents(&files[1]), contents(&fixture.collection()));
    assert_ne!(contents(&files[0]).2, contents(&files[1]).2);
    // a copy stands alone, with no WAL to lose
    assert!(!PathBuf::from(format!("{}-wal", files[1].display())).exists());
    assert!(!files.iter().any(|f| f.to_string_lossy().ends_with(".tmp")));

    // Another start copies the file as it was before opening. Nothing changes, so
    // there's no call for another at shutdown.
    let server = fixture.start();
    assert_eq!(backup_files(&fixture).len(), 3);
    server.stop();
    let files = backup_files(&fixture);
    assert_eq!(files.len(), 3);
    assert_eq!(contents(&files[2]), contents(&fixture.collection()));
}

/// Reviews keep arriving while copies are taken. Each must be complete and
/// consistent, and an interval that's up with no changes takes none.
#[test]
fn backups_taken_during_reviews_are_consistent() {
    let fixture = Fixture::new();
    let seed = fixture.dir.path().join("seed.json");
    let notes: Vec<String> = (0..60)
        .map(|i| format!(r#"{{"notetype": "Basic", "fields": ["q{i}", "a"]}}"#))
        .collect();
    fs::write(&seed, format!(r#"{{"notes": [{}]}}"#, notes.join(","))).unwrap();
    let server = fixture.start_with(&[
        "--create",
        "--seed-file",
        seed.to_str().unwrap(),
        "--backup-interval-secs",
        "1",
    ]);
    // the default limit on new cards a day is too few for this
    let (info, _) = new_per_day(&server);
    let mut config = info.config.clone().unwrap();
    config.config.as_mut().unwrap().new_per_day = 100;
    let _: OpChanges = server.rpc(
        "updateDeckConfigs",
        &UpdateDeckConfigsRequest {
            target_deck_id: 1,
            config: Some(config),
            mode: UpdateDeckConfigsMode::Normal as i32,
            limits: info.current_deck.as_ref().unwrap().limits,
            ..Default::default()
        },
    );
    let start = std::time::Instant::now();
    let mut answered = 0;
    while start.elapsed() < std::time::Duration::from_millis(4500) {
        assert!(answer_next(&server), "ran out of cards");
        answered += 1;
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(answered > 10, "{answered}");

    let during = backup_files(&fixture);
    assert!(during.len() >= 3, "{during:?}");

    // with nothing changing, no more arrive
    std::thread::sleep(std::time::Duration::from_millis(2500));
    let settled = backup_files(&fixture).len();
    std::thread::sleep(std::time::Duration::from_millis(2500));
    assert_eq!(backup_files(&fixture).len(), settled);
    server.stop();

    let files = backup_files(&fixture);
    let mut previous = 0;
    for file in &files {
        quick_check(file);
        let (notes, cards, reviews, _) = contents(file);
        assert_eq!((notes, cards), (60, 60), "{}", file.display());
        // each answer is a whole transaction, and copies are in order
        assert!(reviews >= previous, "{}", file.display());
        previous = reviews;
    }
    assert!(previous >= answered);
    assert_eq!(
        contents(files.last().unwrap()),
        contents(&fixture.collection())
    );
}

#[test]
fn a_failing_backup_is_loud_but_reviewing_carries_on() {
    let fixture = Fixture::new();
    // so the folder can't be made
    fs::write(fixture.dir.path().join("backups"), "in the way").unwrap();
    let mut server =
        fixture.start_with(&["--create", "--seed-demo", "--backup-interval-secs", "1"]);
    assert!(answer_next(&server));
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(answer_next(&server));

    let pid = server.child.id().to_string();
    assert!(Command::new("kill").arg(&pid).status().unwrap().success());
    let mut output = String::new();
    server.stdout.read_to_string(&mut output).unwrap();
    assert!(output.contains("Collection closed"), "{output}");
    let mut errors = String::new();
    server
        .child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut errors)
        .unwrap();
    assert!(errors.contains("BACKUP FAILED"), "{errors}");
    assert!(server.child.wait().unwrap().success());
    // what it could write, it did
    assert_eq!(contents(&fixture.collection()).2, 7);
}

/// The procedure in docs/web-server.md.
#[test]
fn a_backup_can_be_restored() {
    let fixture = Fixture::new();
    let server = fixture.start_seeded();
    assert!(answer_next(&server));
    server.stop();
    let good = backup_files(&fixture).pop().unwrap();
    let good_contents = contents(&good);

    // Work that's to be lost, including from a server that never got to close.
    let server = fixture.start();
    assert!(answer_next(&server));
    let mut server = server;
    server.child.kill().unwrap();
    server.child.wait().unwrap();
    drop(server);

    let collection = fixture.collection();
    for suffix in ["", "-wal", "-shm"] {
        let path = PathBuf::from(format!("{}{suffix}", collection.display()));
        if path.exists() {
            fs::rename(&path, format!("{}{suffix}.broken", collection.display())).unwrap();
        }
    }
    fs::copy(&good, &collection).unwrap();

    let server = fixture.start();
    assert!(answer_next(&server), "reviews carry on from the backup");
    server.stop();
    let (notes, cards, reviews, _) = contents(&collection);
    assert_eq!((notes, cards), (good_contents.0, good_contents.1));
    assert_eq!(reviews, good_contents.2 + 1);
}

/// The owner's collection has this stock notetype, with no notes.
#[test]
fn a_collection_with_the_removed_image_occlusion_notetype_opens() {
    let fixture = Fixture::new();
    let seed = fixture.dir.path().join("seed.json");
    fs::write(
        &seed,
        r#"{"notetypes": [{"name": "Image Occlusion", "fields": ["Occlusion", "Image", "Header",
            "Back Extra", "Comments"], "front": "{{cloze:Occlusion}}<script>anki.setup()</script>",
            "back": "{{cloze:Occlusion}}", "original_stock_kind": 6}]}"#,
    )
    .unwrap();
    let server = fixture.start_with(&["--create", "--seed-file", seed.to_str().unwrap()]);
    server.stop();

    // the pages work on it, and it survives being opened again
    let server = fixture.start();
    let _: DeckConfigsForUpdate = server.rpc("getDeckConfigsForUpdate", &DeckId { did: 1 });
    let _: anki_proto::decks::DeckTreeNode =
        server.rpc("deckTree", &anki_proto::decks::DeckTreeRequest { now: 1 });
    server.stop();
    let db = rusqlite::Connection::open(fixture.collection()).unwrap();
    let mut names = db.prepare("select name from notetypes").unwrap();
    let names: Vec<String> = names
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert!(
        names.iter().any(|name| name == "Image Occlusion"),
        "{names:?}"
    );
}

/// Everything the pages import from the generated client must be on the
/// allow-list, and nothing else.
#[test]
fn allow_list_matches_the_pages() {
    fn walk(dir: &Path, found: &mut std::collections::BTreeSet<String>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, found);
                continue;
            }
            let name = path.to_string_lossy().to_string();
            if !(name.ends_with(".ts") || name.ends_with(".svelte")) || name.contains(".test.") {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            let mut rest = text.as_str();
            while let Some(end) = rest.find("from \"@generated/backend\"") {
                let start = rest[..end].rfind("import").unwrap();
                let names = &rest[start..end];
                let names = &names[names.find('{').unwrap() + 1..names.rfind('}').unwrap()];
                for name in names.split(',') {
                    let name = name.split(" as ").next().unwrap().trim();
                    if !name.is_empty() {
                        found.insert(name.to_string());
                    }
                }
                rest = &rest[end + 1..];
            }
        }
    }
    let ts = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ts");
    let mut used = Default::default();
    for dir in ["routes", "lib", "reviewer"] {
        walk(&ts.join(dir), &mut used);
    }
    let rpc = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/rpc.rs")).unwrap();
    let list = &rpc[rpc.find("ALLOWED: &[&str] = &[").unwrap()..];
    let list = &list[..list.find("];").unwrap()];
    let allowed: std::collections::BTreeSet<String> = list
        .lines()
        .filter_map(|line| line.trim().strip_prefix('"')?.strip_suffix("\","))
        .map(str::to_string)
        .collect();
    assert_eq!(used, allowed);
}

#[test]
fn a_collection_can_only_be_served_once() {
    let fixture = Fixture::new();
    let server = fixture.start();
    let output = fixture.command().output().unwrap();
    assert!(!output.status.success(), "second server started");
    // and the first was unharmed
    let _ = new_per_day(&server);
    server.stop();
    assert!(Path::new(&fixture.collection()).exists());
}

#[test]
fn seed_demo_gives_cards_and_reviews() {
    let fixture = Fixture::new();
    let server = fixture.start_seeded();
    let graphs: anki_proto::stats::GraphsResponse = server.rpc(
        "graphs",
        &anki_proto::stats::GraphsRequest {
            search: "".into(),
            days: 365,
        },
    );
    assert_eq!(graphs.today.unwrap().answer_count, 5);
    server.stop();
}

/// Everything the next milestone's reviewer needs, in the order it'd use it.
#[test]
fn reviewer_rpcs_work() {
    use anki_proto::card_rendering as render;
    use anki_proto::collection::OpChangesAfterUndo;
    use anki_proto::collection::UndoStatus;
    use anki_proto::decks::DeckTreeNode;
    use anki_proto::decks::DeckTreeRequest;
    use anki_proto::scheduler::card_answer::Rating;
    use anki_proto::scheduler::queued_cards::QueuedCard;
    use anki_proto::scheduler::CardAnswer;
    use anki_proto::scheduler::GetQueuedCardsRequest;
    use anki_proto::scheduler::QueuedCards;

    let fixture = Fixture::new();
    let server = fixture.start_seeded();

    let tree: DeckTreeNode = server.rpc("deckTree", &DeckTreeRequest { now: 1 });
    assert_eq!(tree.children[0].name, "Default");
    let _: OpChanges = server.rpc("setCurrentDeck", &DeckId { did: 1 });

    let next_card = || -> QueuedCard {
        let queued: QueuedCards = server.rpc(
            "getQueuedCards",
            &GetQueuedCardsRequest {
                fetch_limit: 1,
                intraday_learning_only: false,
            },
        );
        queued.cards.into_iter().next().unwrap()
    };
    let queued = next_card();
    let card_id = queued.card.as_ref().unwrap().id;

    let rendered: render::RenderCardResponse = server.rpc(
        "renderExistingCard",
        &render::RenderExistingCardRequest {
            card_id,
            browser: false,
            partial_render: false,
        },
    );
    assert!(format!("{:?}", rendered.question_nodes).contains("Question"));
    let tags: render::ExtractAvTagsResponse = server.rpc(
        "extractAvTags",
        &render::ExtractAvTagsRequest {
            text: "x[sound:a.mp3]".into(),
            question_side: true,
        },
    );
    assert_eq!(tags.av_tags.len(), 1);
    let compared: generic::String = server.rpc(
        "compareAnswer",
        &render::CompareAnswerRequest {
            expected: "abc".into(),
            provided: "abd".into(),
            combining: true,
            ignore_case: false,
        },
    );
    assert!(!compared.val.is_empty());

    let states = queued.states.unwrap();
    let described: generic::StringList = server.rpc("describeNextStates", &states);
    assert_eq!(described.vals.len(), 4);

    let _: OpChanges = server.rpc(
        "answerCard",
        &CardAnswer {
            card_id,
            current_state: states.current,
            new_state: states.good,
            rating: Rating::Good as i32,
            answered_at_millis: 1_700_000_000_000,
            milliseconds_taken: 1000,
        },
    );
    let status: UndoStatus = server.rpc("getUndoStatus", &generic::Empty {});
    assert!(!status.undo.is_empty());
    let _: OpChangesAfterUndo = server.rpc("undo", &generic::Empty {});
    let status: UndoStatus = server.rpc("getUndoStatus", &generic::Empty {});
    assert!(!status.redo.is_empty());

    server.stop();
}
