# Local web server

`evolanki-server` (`rslib/web-server`) serves a collection to a browser on the
same machine. It links the Rust backend directly. Building it needs Rust and Node,
which the build downloads, and what [development.md](development.md) lists.

## Run

```
tools/run-web
```

This builds what it needs and serves a demo collection from `out/web-dev` at
<http://127.0.0.1:8765>: choose a deck, then study it. Ctrl+C closes the collection
cleanly.

To serve another collection, `tools/run-web path/to/file.anki2`. Media comes from
the matching `.media` folder, as in a desktop profile. Don't point it at a
collection you care about yet: there is no backup, and opening one migrates it
to FSRS.

The repo path must not contain a space, as `./ninja` can't handle that.

## Build by hand

```
./ninja web
CARGO_TARGET_DIR=$PWD/out/rust cargo build -p evolanki-server
out/rust/debug/evolanki-server --collection /tmp/test/col.anki2 --create --seed-demo
```

- `--collection PATH`: required. Must exist unless `--create` is given.
- `--create`: make a new collection at that path if there's none.
- `--seed-demo`: add 20 notes and 5 reviews to a newly created collection.
- `--seed-file FILE`: add the decks and notes in this JSON file to a newly created
  collection instead. For tests; see `SeedFile` in `src/seed.rs`.
- `--port N`: default 8765 (also AnkiConnect's default); 0 picks a free port.
- `--web-root DIR`: default `out/web`, relative to the current folder.
- `--lang CODE`: default `en`.

## Security

There's no login, so the server trusts only what it can show came from its own
pages. It binds to 127.0.0.1 and refuses to do otherwise. All requests need a Host
of exactly `127.0.0.1:PORT` or `localhost:PORT`, and an Origin matching that Host
if one is sent. POSTs also need `Content-Type: application/binary` and can't be
marked cross-site by `Sec-Fetch-Site`. Only the RPCs in `src/rpc.rs` can be called.

The server is not protected from a local process, or from another user of the
machine, which can connect to 127.0.0.1 and send any headers it likes.

### Card content and deck descriptions

Cards can come from anyone's shared deck, so their HTML and scripts are untrusted.
They run in an iframe with `sandbox="allow-scripts"` and no `allow-same-origin`
(`ts/lib/components/SandboxedFrame.svelte`), loading the document in
`src/card_frame.html`. In layers:

- The frame's origin is opaque, so it can't read the parent, and the server's Origin
  check refuses its API calls.
- Its CSP has `connect-src 'none'`, and images, media, fonts and external scripts
  only from `/_frame/<token>/`. The CSP repeats the sandbox, in case the document is
  opened directly. Remote images and other outside content don't load.
- The token is random per run. That prefix alone has `Cross-Origin-Resource-Policy:
  cross-origin` and `Access-Control-Allow-Origin: null`, so a sandboxed frame can
  load media, while other sites still can't embed it, and it holds only media and the
  few assets in `is_frame_asset()`. It answers nothing but GET and HEAD.
- The page gets the prefix from `/_anki/frameBase`, which refuses cross-site callers.
- The page and frame talk over a private `MessagePort`, handed over before there is
  any card code in the frame, which then only has it in a closure. The page doesn't
  listen to window messages at all. Messages are validated
  (`ts/lib/tslib/frame-protocol.ts`), and the frame forwards keys only from events
  with `isTrusted`. So a card can't answer, undo or suspend anything; `pycmd()` in a
  card can only play its own audio or reveal its answer.
- Pages that embed frames have `frame-src 'self'`, which blocks a card sending its
  frame to another site. If a frame loads anything a second time, it's replaced and
  gets a new port, and a card that keeps doing this isn't shown.

Deck descriptions use the same document with `?static`, whose scripts need a nonce
that the content can't know, so nothing in them runs, inline handlers included.

The frame can still make requests that no CSP covers, such as navigating itself or
WebRTC, so a card could send its own content somewhere. It has no access to anything
else in the collection. Keep that in mind when a card shows something sensitive.

A card's `localStorage` throws and dialogs do nothing, because of the sandbox.

## Differences from desktop Anki

- Custom scheduling isn't run: `getSchedulingStatesWithContext` and
  `setSchedulingStates` still answer 501, card scripts can't reach them, and deck
  options hides the setting. The stored value is kept.
- TTS tags and video are not played, and `[latex]` isn't rendered. Flags, marks,
  timeboxing, auto advance, `reviewExtra`, editing and card info aren't in the reviewer.

## Differences from desktop Anki's mediasrv.py

- `updateDeckConfigs` returns when the save is done, with the real output.
  mediasrv.py started a Qt background op, returned nothing, and closed the dialog
  on success. The page shows progress itself, then returns to the deck.
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
