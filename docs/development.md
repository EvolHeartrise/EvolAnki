# Development

## Requirements

Clone the repo into a folder of your choosing. The path must not
contain spaces, and should not be too long if you are on Windows.

You will need:

- Rustup (https://rustup.rs/). The Rust version pinned in rust-toolchain.toml
  will be automatically downloaded if not yet installed.
- N2 or Ninja. N2 gives better status output. You can install it with `tools/install-n2`,
  or `bash tools\install-n2` on Windows. Ninja can be downloaded from
  https://github.com/ninja-build/ninja/releases/tag/v1.11.1 and placed on your path, or
  from your distro/homebrew if it's 1.10+.
- rsync, git, and a C compiler for the bundled SQLite and zstd.
- (Optional) [just](https://just.systems/man/en/packages.html), a command runner.

Node is downloaded by the build, along with protoc and the JavaScript dependencies.
Python is not used.

## Running

```
tools/run-web
```

See [web-server.md](web-server.md).

## Tests and checks

To run all of them, from the top-level folder:

```
./ninja check
```

(`tools\ninja check` on Windows).

You can also run specific checks, such as `./ninja check:vitest`, `./ninja check:svelte`
or `./ninja check:clippy`. Targets are hierarchical, so `./ninja check:format` runs all
the format checks.

Don't ask a single ninja invocation for both `check:svelte` and the web build. The
former regenerates `ts/.svelte-kit`, which has broken the bundle when they overlapped.

The browser tests are separate:

```
./ninja web
CARGO_TARGET_DIR=$PWD/out/rust cargo build -p evolanki-server
out/extracted/node/bin/node node_modules/.bin/playwright test -c playwright.web.config.ts
```

## Fixing problems

```
./ninja format        # formatting
./ninja fix           # eslint
tools/update-licenses # cargo/licenses.json, ts/licenses.json
cargo clippy --fix    # clippy
```

## Optimized builds

Set RELEASE=1 or RELEASE=2 for an optimized build. The latter will further optimize the
output, but make the build much slower.

## Cleaning up build files

Most build files go into the `out/` folder (and
`node_modules` on Windows). You can delete that folder for a clean build, or
to free space. `tools/clean` keeps downloads, and with `keep-env`, also node_modules.

If you invoke Rust outside of the build scripts (eg by running cargo, or
with Rust Analyzer), output files will go into `target/` unless you have
overridden the default output location.

## Environment variables

If TRACESQL is set, all SQL statements will be printed as they are executed.
