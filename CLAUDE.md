# Claude Code Configuration

> **Note:** Every command you need — building, running, testing, linting,
> formatting — is defined as a recipe in the project `justfile`. Run
> `just --list` to see them.

## Project Overview

EvolAnki is a slim, FSRS-only spaced repetition web app, modified from Anki. Main components:

- Web frontend: Svelte/TypeScript in ts/
- Core Rust layer in rslib/
- `evolanki-server` in rslib/web-server, which serves the pages and backend (docs/web-server.md)
- `evolanki-admin`, also there, the JSON command-line tool for managing cards (docs/admin-cli.md)
- Protobuf definitions in proto/ that the layers use to talk to each other.

## Running

```
just run
```

This builds the pages and server, then serves a demo collection, on the network as well
as at http://127.0.0.1:8765. Only use throwaway collections.

## Building/checking

`just check` will format the code and run the main build & checks.
Please do this as a final step before marking a task as completed.

Run `just` (or `just --list`) to see all available commands.

## Quick iteration

During development, you can build/check subsections of our code:

- Rust: `cargo check`
- TypeScript/Svelte: `just lint` (includes check:svelte)

Language-specific tests are also available: `just test-rust`, `just test-ts`.
Use `just fmt` / `just fix-fmt` for formatting and `just fix-lint` to auto-fix
lint issues.

Changes to .proto files reach TypeScript once `just gen` (run by `just build`,
`just lint` and `just test-ts`) regenerates out/ts/lib/generated.

Don't run `just lint` (svelte-check) while `just build` is running, as that
has caused a broken bundle.

## Testing guidance

Before adding or changing unit or component tests, read and follow
[Writing Unit Tests](docs/unit-testing.md).

## Build tooling

The `justfile` is the build: its recipes run cargo, yarn (through corepack),
esbuild and Sass. Node 22 or 24 must be installed. See docs/build.md.

## User-visible strings

The app is English-only. Strings are plain hand-written functions: in Rust,
methods on `I18n` in rslib/src/i18n.rs (called as `tr.some_string(args)`); in
TypeScript, functions in ts/lib/tslib/tr.ts (imported as
`import * as tr from "@tslib/tr"`). Add new strings there, matching the
existing style.

## Protobuf and IPC

Our build scripts use the .proto files to define our Rust library's
non-Rust API. A @generated/backend TypeScript module communicates with the
Rust backend over POST requests, which rslib/web-server/src/rpc.rs allow-lists.

## Fixing errors

When dealing with build errors or failing tests, invoke 'check' or one
of the quick iteration commands regularly. This helps verify your changes
are correct. To locate other instances of a problem, run the check again -
don't attempt to grep the codebase.

## Ignores

The files in out/ are auto-generated. Mostly you should ignore that folder,
though you may sometimes find it useful to view out/ts/lib/generated when dealing
with cross-language communication or our other generated sourcecode.

## Rust dependencies

Prefer adding to the root workspace, and using dep.workspace = true in the individual Rust project.

## Rust utilities

rslib/io contains some helpers for file operations, which provide better
error messages/context and some ergonomics. Use them when possible.

## Rust error handling

in rslib, use error/mod.rs's AnkiError/Result and snafu. In our other Rust modules, prefer anyhow + additional context where appropriate. Unwrapping
in build scripts/tests is fine.

## Individual preferences

See @.claude/user.md
