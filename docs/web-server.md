# Web server

`evolanki-server` (`rslib/web-server`) serves a collection to browsers on your network,
including your phone. It links the Rust backend directly. Building it needs Rust and
Node, which the build downloads, and what [development.md](development.md) lists.

## Run

```
tools/run-web --release path/to/collection.anki2
```

This builds the pages and an optimised server, and serves that collection. It prints the
addresses to open: type one into your phone's browser. Media comes from the matching
`.media` folder, as in a desktop profile. Ctrl+C takes a last backup and closes the
collection cleanly. Opening a collection migrates it to FSRS.

Leave out `--release` for a quicker debug build, and the path for a demo collection in
`out/web-dev`. It runs from any folder, and the repo path must not contain a space, as
`./ninja` can't handle that.

## Install to a stable path

`tools/install-local` builds the release, then puts the server, `evolanki-admin` and the
pages in `~/.local/share/evolanki`, and prints the command to run them. It works from a
checkout whose path has a space, by building a clone of HEAD in `~/.cache/evolanki-build`.
It touches no collection, and installs no service.

## Build by hand

```
./ninja web
CARGO_TARGET_DIR=$PWD/out/rust cargo build --release -p evolanki-server
out/rust/release/evolanki-server --collection /tmp/test/col.anki2 --create --seed-demo
```

- `--collection PATH`: required. Must exist unless `--create` is given.
- `--create`: make a new collection at that path if there's none.
- `--seed-demo`: add 20 notes and 5 reviews to a newly created collection.
- `--seed-file FILE`: add the decks and notes in this JSON file to a newly created
  collection instead. For tests; see `SeedFile` in `src/seed.rs`.
- `--host IP`: the address to listen on; by default `0.0.0.0`, every interface.
  `--host 127.0.0.1` keeps it to this machine.
- `--allow-host NAME`: another name or IP to accept in addresses, such as a VPN's. Can
  be repeated.
- `--port N`: default 8765 (also AnkiConnect's default); 0 picks a free port.
- `--web-root DIR`: the built pages. By default `out/web` in the build tree that holds
  the binary, wherever you run it from. Pass this if you move the binary.
- `--backup-interval-secs N`: the least time between backups while reviewing; default
  1800.

While it runs, `evolanki-admin` reaches the collection through a private Unix socket
beside it, `collection.admin.sock`: see [admin-cli.md](admin-cli.md).

## No login: what that means

**Anyone who can connect to this server can read and change everything in this
collection.** That means every device on your network: other people's phones on the
same Wi-Fi, a visitor's laptop, or anything compromised. Nothing is encrypted either, so
someone on the network can watch your reviews go by. Only run it on a network you trust.
Never forward its port from a router, or use it on public Wi-Fi. The owner chose this
over a password, knowing that, and it prints a warning each time it starts.

The server's API is narrow: it can only do what the pages do. It can't delete notes or
reach files outside the collection's media, but it can answer and undo reviews, and
change deck options. Anything more goes through [`evolanki-admin`](admin-cli.md) over a
Unix socket that only this user can open, and never over the network.

## Security

So what remains is keeping *other web sites* out. A page you visit on the internet can
make your browser send requests to this server, and these checks stop that:

- **Host** must be a loopback address, `localhost`, a private or link-local IP address
  (10.x, 172.16-31.x, 192.168.x, 169.254.x, or IPv6's `fc00::/7` and `fe80::/10`), this
  machine's name with or without `.local`, or an `--allow-host`, always with the right
  port. This stops DNS rebinding, where evil.example resolves to your machine and so
  counts as the same site in your browser. A public IP or any other name gets a 403. So
  a VPN or `100.x` address needs `--allow-host`.
- **Origin**, if sent, must match Host. Browsers always send it with a POST.
- POSTs also need `Content-Type: application/binary`, which another site can't send
  without a preflight that's never granted, and can't be marked cross-site by
  `Sec-Fetch-Site` (which browsers leave out on plain HTTP LAN addresses, as they aren't
  "secure").
- Only the RPCs in `src/rpc.rs` can be called, and they're exactly what the pages import,
  which a test checks. The rest get a 404, including everything that names server paths.

None of this stops another device, or a local process, from connecting and sending any
headers it likes. That's the risk described above.

### Card content and deck descriptions

Cards can come from anyone's shared deck, so their HTML is untrusted, and **scripts in
cards don't run at all**, unlike in desktop Anki. Templates with their own JavaScript show
as if it wasn't there. In layers:

- The iframe has `sandbox="allow-scripts"` and no `allow-same-origin`
  (`ts/lib/components/SandboxedFrame.svelte`), loading the document in
  `src/card_frame.html`. The scripts are for our own loader.
- Its CSP allows only scripts with a nonce made for that response, and MathJax's files
  under `a/js/`. There's no `unsafe-inline`, so `<script>`, `onclick`, `onerror` and
  `javascript:` links are all refused. There's no `unsafe-eval` either. The reviewer
  sets content with `innerHTML`, which doesn't start scripts by itself.
- Should something get past that, the frame's origin is opaque, so it can't read the
  parent, and the server's Origin check refuses its API calls.
- The CSP has `connect-src 'none'`, and images, media and fonts only from
  `/_frame/<token>/`, repeating the sandbox in case the document is opened directly.
  Remote images and other outside content don't load.
- The token is random per run. That prefix alone has `Cross-Origin-Resource-Policy:
  cross-origin` and `Access-Control-Allow-Origin: null`, so a sandboxed frame can
  load media, while other sites still can't embed it, and it holds only media and the
  few assets in `is_frame_asset()`. It answers nothing but GET and HEAD.
- The page gets the prefix from `/_anki/frameBase`, which refuses cross-site callers.
- The page and frame talk over a private `MessagePort`, handed over before there is
  any card content in the frame, which then only has it in a closure. The page doesn't
  listen to window messages at all. Messages are validated
  (`ts/lib/tslib/frame-protocol.ts`), and the frame forwards keys and clicks only from
  events with `isTrusted`.
- Pages that embed frames have `frame-src 'self'`, which blocks a card sending its
  frame to another site, with `<meta http-equiv="refresh">`, the one way it can still
  move without a script. If a frame loads anything a second time, it's replaced and
  gets a new port, and a card that keeps doing this isn't shown.

What cards relied on inline handlers for is handled by the frame: play buttons and the
`{{hint:}}` filter's link are data attributes for a click listener.

A card can still send what it contains to another site by being clicked: following a
link opens in a new tab, as a link should. Nothing else in the collection is reachable.

## Backups

The server keeps copies of the collection in a `backups` folder beside it, named like
`col-2026-11-01-05.30.12.anki2`. There's no UI and nothing to set up. It takes one:

- at every start, before opening the collection, so before anything can upgrade it;
- every 30 minutes at most while there have been changes;
- at shutdown, if something changed since the last.

They're made with SQLite's online backup API on the collection's own connection, while no
operation is running, and each is checked with `quick_check` before it gets its final
name. A live file is never just copied. A review waits for the moment that takes.

**Kept:** the latest 48, plus the last one of each of the latest 30 days that have any,
so at most 78. Older ones are deleted, as are temporary files from a crash; nothing
else in that folder is touched. Each is the size of the collection.

**If a backup fails**, reviewing carries on, and it says `BACKUP FAILED` on stderr each
time. Check that output if you run it unattended, for a full disk especially.

**Media isn't included.** `evolanki-admin` can add media, overwrite it and import
packages that bring more, so the `.media` folder needs its own copy, taken again after
those. Copy it to another disk, along with these. All of this is on one disk, so it
doesn't protect you from losing that. `media.replace` keeps what it overwrites in
`backups/media-replaced`, which is never pruned.

`evolanki-admin` also takes a backup before destructive or bulk commands, which count
towards the same limits.

### Restoring

1. Stop the server.
2. Move the broken collection out of the way, with its `-wal` and `-shm` files if they
   exist. It's essential not to leave those behind, as they'd be replayed into the
   restored copy:
   ```
   mv col.anki2 col.anki2.broken
   mv col.anki2-wal col.anki2-wal.broken
   mv col.anki2-shm col.anki2-shm.broken
   ```
3. Copy the backup you want into its place. Copy, so that the backup stays:
   ```
   cp backups/col-2026-11-01-05.30.12.anki2 col.anki2
   ```
4. Start the server. It takes a new backup of what you restored, and reviews carry on
   from there. `a_backup_can_be_restored` in `tests/http.rs` follows these steps.

Backups are ordinary collections, so `sqlite3` or a desktop Anki can read them too.

## Phones

The pages are laid out for a phone: 44px targets, answer buttons along the bottom edge,
room for a notch, and no sideways scrolling. Add it to your home screen if you like.

Phones won't play sound until you've tapped something, which the reviewer deals with:
your tap on Study Now also starts its audio element, which then may play freely. If
it's refused anyway, such as after reloading the reviewer, a **Tap to play sound** button
appears and plays what was held back.

`ts/tests/web/mobile.spec.ts` runs all this in Playwright's Chromium, with Chrome's
real autoplay policy, and its WebKit, with emulated devices. WebKit on Linux has no
autoplay policy to speak of, so that one is acted out with a stand-in that refuses
until a tap. **None of this has been on an actual phone.** Things to check on one: that
autoplay works after Study Now on iOS Safari in particular (it depends on the one-tap
"unlock" above), the on-screen keyboard over type-in answers, safe-area insets on a notch, and how your audio formats play. iOS has no `.ogg`/`.opus` support before
Safari 18.4. A file that can't play is skipped.

## Differences from desktop Anki

- Scripts in cards don't run, so nor does custom scheduling.
- `[sound:]` files play in the page, one after another. Only audio plays: mp3, m4a, ogg,
  oga, opus, wav, flac and aac. Anything else, such as `[sound:x.mp4]`, isn't requested,
  and the reviewer says so in a notice while the card carries on. There's no video player.
- There is no text-to-speech. A `{{tts ...:Field}}` filter or `[anki:tts]` tag in an
  imported card shows nothing, and plays nothing. Their text was only ever spoken, so a
  card whose front was only that has a blank front.
- English only.
- `[latex]` isn't rendered. Marks, timeboxing, auto advance, `reviewExtra`, editing
  and suspending aren't in the reviewer. Undo and type-in answers are. A card suspended
  while it's on screen refuses the answer, with a notice, and the next card shows.
- There is no burying, and nothing replaces it: cards of one note can all come up on the
  same day, and the load balancer doesn't space them apart. Queues -2 and -3 are gone, so
  `is:buried` is a search error. Opening a collection returns any card still in those
  queues to normal, without changing anything else about it. So do `evolanki-admin
  convert` and importing, as they open what they read.
- There are no flags. `flag:` isn't a search keyword any more, so `flag:1` looks for "1" in
  a field called flag. A new card's `flags` column is 0, and nothing reads it. `{{CardFlag}}`
  in a template is empty.
- There are no leeches. However many lapses a card has, it isn't tagged or suspended, and
  the simulator doesn't model that. A `leech` tag is just a tag.
- There's no congratulations page. When a deck runs out, the reviewer returns to the deck
  list, showing it with nothing left. Its overview page shows zero counts and no Study Now,
  and doesn't say when the next learning card is due, or that a daily limit was reached.
- No pages for: adding or editing cards, the browser, changing a notetype, importing,
  making filtered decks, custom study, or managing decks and tags. Agents do those with
  [`evolanki-admin`](admin-cli.md). They aren't reachable over HTTP.
- Deck options has no easy days section. Stored values are used as before; with none,
  every day counts as normal. The load balancer has no control either, and stays on
  unless the collection's config says otherwise.

## Differences from desktop Anki's mediasrv.py

- `updateDeckConfigs` returns when the save is done, with the real output.
  mediasrv.py started a Qt background op, returned nothing, and closed the dialog
  on success. The page shows progress itself, then returns to the deck.
- The `Anki-Op-Changes` header is ignored. It drove on_op_finished() in Qt.
- `/_addons/` isn't served, and neither are the legacy `/_anki/foo.js` remappings.

## Tests

```
CARGO_TARGET_DIR=$PWD/out/rust cargo test -p evolanki-server
out/extracted/node/bin/node node_modules/.bin/playwright test -c playwright.web.config.ts
```

Both create throwaway collections in temporary folders. The Playwright tests need the
assets from above, a debug server binary, and Chromium and WebKit
(`playwright install chromium webkit`). Set `EVOLANKI_SHOTS=folder` for screenshots.
