# Development

## Requirements

Clone the repo into a folder of your choosing. This is developed and run on Linux.

You will need:

- Rustup (https://rustup.rs/). The Rust version pinned in rust-toolchain.toml
  will be automatically downloaded if not yet installed.
- Node 22 or 24, which include corepack. The build runs yarn through corepack, which
  downloads the version that package.json names.
- [just](https://just.systems/man/en/packages.html), which runs the build commands.
- git, and a C compiler for the bundled SQLite and zstd.

Python is not used. `cargo build` and `cargo check` work by themselves, as the build
compiles the protobuf definitions without protoc.

## Running

```
just run
```

See [web-server.md](web-server.md).

## Tests and checks

To format the code, then build and run all the checks:

```
just check
```

`just --list` shows the others, such as `just fmt` (format check), `just lint` (clippy,
eslint and svelte-check), `just test-rust` and `just test-ts`. The Rust tests run with
[cargo-nextest](https://nexte.st/), which `just test-rust` installs if it's missing.

Don't run `just lint` while `just build` is running. svelte-check regenerates
`ts/.svelte-kit`, which has broken the bundle when they overlapped.

## Fixing problems

```
just fix-fmt          # formatting
just fix-lint         # eslint
just update-licenses  # cargo/licenses.json, ts/licenses.json
cargo clippy --fix    # clippy
```

## Optimized builds

`tools/run-web --release` builds an optimized server. The pages are always minified.

## Cleaning up build files

Build files go into the `out/` folder, including cargo's, and the JavaScript dependencies
into `node_modules`. `just clean` removes both, and with `keep-env`, keeps
`node_modules`.

## Environment variables

If TRACESQL is set, all SQL statements will be printed as they are executed.
