# The build system

## Basic use

See [development.md](development.md).

## How it fits together

The `justfile` holds the build: each recipe is a few plain commands, so `just --dry-run
build` shows what a build runs. Cargo builds the Rust, and yarn installs the JavaScript
dependencies and runs Vite. Build outputs go into `out/`.

- `deps` runs `yarn install` when `package.json` or `yarn.lock` is newer than the last
  install. Yarn comes from Node's corepack, at the version `package.json` names.
- `gen` builds `rslib/proto` with cargo. Its build script compiles `proto/` with
  [protox](https://github.com/andrewhickman/protox), a protobuf compiler in Rust, so no
  protoc is needed. It writes `out/rslib/proto/descriptors.bin`, which the other crates
  read through `DESCRIPTORS_BIN` in `.cargo/config.toml`, and the pages' backend calls,
  `out/ts/lib/generated/backend.ts`. Then `ts/tools/genproto.mjs` passes the descriptors
  to protoc-gen-es for the TypeScript messages. `@generated` in the pages' imports is
  `out/ts/lib/generated`.
- `build` makes `out/web`, the folder the server serves: Vite builds the SvelteKit pages
  into `out/web/sveltekit`, esbuild bundles the reviewer's script and MathJax setup, Sass
  compiles the card and reviewer CSS, and the MathJax files the cards use are copied in.

Cargo's target directory is `out/rust`, set in `.cargo/config.toml`, so plain `cargo`
commands share it with the recipes.

The build supports Linux only, on x86-64 or arm64.

## Tracing build problems

- `just --dry-run RECIPE` prints the commands a recipe would run.
- `just clean` removes `out/` and `node_modules`, for a clean build.
