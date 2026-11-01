# End-to-End Testing with Playwright

Playwright drives pages served by a real `evolanki-server`. Tests live in
`ts/tests/web/` and are entirely separate from the Vitest unit tests. Each test starts
the server itself on a throwaway collection in a temporary folder
(see `ts/tests/web/server.ts`), so a test can also restart it.

## Running

```shell
./ninja web
CARGO_TARGET_DIR=$PWD/out/rust cargo build -p evolanki-server
out/extracted/node/bin/yarn playwright install chromium   # first time only
out/extracted/node/bin/node node_modules/.bin/playwright test -c playwright.web.config.ts
```

Or `just test-e2e`, which does all of this. Results and traces of failures are written
to `out/e2e-web-results/`.

## Writing tests

Use `tempCollection()` and `Server.start()` from `server.ts`, and go to pages
through `server.url`. `--seed-file` sets up the decks and notes a test needs; see
`SeedFile` in `rslib/web-server/src/seed.rs`.

`failOnPageProblems()` in `pages.spec.ts` fails a test on anything a user would see go
wrong, such as a dialog or a console error.

See [Writing Unit Tests](unit-testing.md) for choosing what's worth testing here rather
than in a faster test.
