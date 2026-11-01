# Local web server

`evolanki-server` (`rslib/web-server`) serves a collection to a browser on the
same machine. It links the Rust backend directly; Python and Qt are not used at
build or run time, beyond what the pages' build scripts do.

## Run

```
tools/run-web
```

This builds what it needs and serves a demo collection from `out/web-dev` at
<http://127.0.0.1:8765>. Try `/deck-options/1`, `/graphs`, `/congrats` and
`/card-info/<card id>`. There is no home page yet. Ctrl+C closes the collection
cleanly.

To serve another collection, `tools/run-web path/to/file.anki2`. Media comes from
the matching `.media` folder, as in a desktop profile. Don't point it at a
collection you care about yet: there is no backup, and opening one migrates it
to FSRS.

The repo path must not contain a space, as `./ninja` can't handle that.

## Build by hand

```
./ninja qt:aqt:data:web:sveltekit qt:aqt:data:web:css qt:aqt:data:web:imgs \
    qt:aqt:data:web:js qt:aqt:data:web:js:vendor
CARGO_TARGET_DIR=$PWD/out/rust cargo build -p evolanki-server
out/rust/debug/evolanki-server --collection /tmp/test/col.anki2 --create --seed-demo
```

- `--collection PATH`: required. Must exist unless `--create` is given.
- `--create`: make a new collection at that path if there's none.
- `--seed-demo`: add 20 notes and 5 reviews to a newly created collection.
- `--port N`: default 8765 (also AnkiConnect's default); 0 picks a free port.
- `--web-root DIR`: default `out/qt/_aqt/data/web`, relative to the current folder.
- `--lang CODE`: default `en`.

## Security

There's no login, so the server trusts only what it can show came from its own
pages. It binds to 127.0.0.1 and refuses to do otherwise. All requests need a Host
of exactly `127.0.0.1:PORT` or `localhost:PORT`, and an Origin matching that Host
if one is sent. POSTs also need `Content-Type: application/binary` and can't be
marked cross-site by `Sec-Fetch-Site`. Only the RPCs in `src/rpc.rs` can be called.

The server is not protected from a local process, or from another user of the
machine, which can connect to 127.0.0.1 and send any headers it likes.

## Differences from mediasrv.py

- `updateDeckConfigs` returns when the save is done, with the real output.
  mediasrv.py started a Qt background op, returned nothing, and closed the dialog
  on success. Nothing closes or reports success on the web, and the progress dialog
  Qt showed for this call has no replacement; `latestProgress` is there to poll.
- `congratsInfo` no longer moves a main window to the overview, and reports
  `bridgeCommandsSupported: false`, as those links need Qt.
- The `Anki-Op-Changes` header is ignored. It drove on_op_finished() in Qt.
- FrontendService methods that drive Qt answer 501 with an explanation; see
  `UNSUPPORTED` in `src/rpc.rs`. `deckOptionsReady` succeeds and does nothing.
- RPCs that name server paths or fetch URLs are not exposed; see `ALLOWED`.
- `/_addons/` isn't served, and neither are the legacy `/_anki/foo.js` remappings.

## Tests

```
CARGO_TARGET_DIR=$PWD/out/rust cargo test -p evolanki-server
out/extracted/node/bin/node node_modules/.bin/playwright test -c playwright.web.config.ts
```

Both create throwaway collections in temporary folders. The Playwright tests
need the assets from above and a built server binary.
