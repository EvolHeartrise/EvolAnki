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
use anki_proto::scheduler::CongratsInfoResponse;
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
            .args(["--port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    fn start(&self) -> Server {
        let mut child = self.command().args(["--create"]).spawn().unwrap();
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
    let value = info.all_config[0]
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

    let mut config = info.all_config[0].config.clone().unwrap();
    config.config.as_mut().unwrap().new_per_day = wanted;
    let _: OpChanges = server.rpc(
        "updateDeckConfigs",
        &UpdateDeckConfigsRequest {
            target_deck_id: 1,
            configs: vec![config],
            mode: UpdateDeckConfigsMode::Normal as i32,
            limits: info.current_deck.as_ref().unwrap().limits,
            fsrs: true,
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
fn rpc_behaviour_matches_mediasrv() {
    let fixture = Fixture::new();
    let server = fixture.start();

    // successful call with no output is a 204
    assert_eq!(server.rpc_raw("deckOptionsReady", b"").status, 204);

    // binary output
    let reply = server.rpc_raw(
        "getConfigJson",
        &generic::String {
            val: "schedVer".into(),
        }
        .encode_to_vec(),
    );
    assert_eq!(reply.status, 200);
    assert_eq!(
        reply.header("content-type").as_deref(),
        Some("application/binary")
    );

    // a missing config key is null, not an error
    let json: generic::Json = server.rpc(
        "getConfigJson",
        &generic::String {
            val: "no such key".into(),
        },
    );
    assert_eq!(json.json, b"null");

    // backend errors are a 500 with the message as plain text
    let reply = server.rpc_raw("getCard", &DeckId { did: 123 }.encode_to_vec());
    assert_eq!(reply.status, 500);
    assert!(!reply.text().is_empty());
    let reply = server.rpc_raw("getDeckConfigsForUpdate", b"\xff\xff\xff");
    assert_eq!(reply.status, 500);

    // the congrats page must not offer Qt-only links
    let info: CongratsInfoResponse = server.rpc("congratsInfo", &generic::Empty {});
    assert!(!info.bridge_commands_supported);
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
        "doesNotExist",
    ] {
        assert_eq!(server.rpc_raw(method, b"").status, 404, "{method}");
    }
    // still open
    let _: DeckConfigsForUpdate = server.rpc("getDeckConfigsForUpdate", &DeckId { did: 1 });

    // Qt-only methods explain themselves
    let reply = server.rpc_raw("openFilePicker", b"");
    assert_eq!(reply.status, 501);
    assert!(reply.text().contains("not available in the web version"));

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

    // ranges are needed for audio/video seeking
    let host = server.host();
    let range = [("Host", host.as_str()), ("Range", "bytes=2-4")];
    let reply = server.raw("GET", "/sp%20ace%20%C3%A9.txt", &range, b"");
    assert_eq!(reply.status, 206);
    assert_eq!(reply.text(), "234");

    assert_eq!(server.get("/missing.png").status, 404);
    assert_eq!(server.get("/folder").status, 403);
    assert_eq!(server.get("/").status, 404);

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

    for trusted in ["/deck-options/1", "/graphs", "/card-info/5", "/congrats"] {
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

    let reply = server.get("/editor/");
    let csp = reply.header("content-security-policy").unwrap();
    assert!(csp.contains("script-src http://127.0.0.1:"), "{csp}");
    assert!(csp.contains("'sha256-abc='"), "{csp}");
    assert!(reply.text().contains("content-security-policy"));

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

fn fails_with(fixture: &Fixture, args: &[&str], message: &str) {
    let output = fixture.command().args(args).output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(message), "{stderr}");
}

#[test]
fn refuses_unsafe_startup() {
    let fixture = Fixture::new();
    for host in ["0.0.0.0", "::", "192.168.1.1", "localhost"] {
        fails_with(&fixture, &["--create", "--host", host], "refusing to bind");
    }
    assert!(!fixture.collection().exists());

    fails_with(&fixture, &[], "pass --create");
    assert!(!fixture.collection().exists());

    let output = Command::new(binary())
        .arg("--collection")
        .arg(fixture.collection())
        .args(["--create", "--port", "0", "--web-root", "/nonexistent"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not found"));
    assert!(!fixture.collection().exists());
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
    let mut child = fixture
        .command()
        .args(["--create", "--seed-demo"])
        .spawn()
        .unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let server = Server {
        port: line.trim().rsplit(':').next().unwrap().parse().unwrap(),
        child,
        stdout,
    };
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
