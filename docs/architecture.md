# Architecture

EvolAnki is a local, single-user web app: a Rust server that links Anki's Rust
backend directly, and Svelte pages that talk to it.

## Backend

Nearly all logic lives in the Rust library in `rslib/`: collections, the scheduler
and FSRS, importing and exporting, and so on. Filtered decks and custom study are in
the backend, and have no UI of their own.

`rslib/web-server` is `evolanki-server`, which serves the pages and exposes an
allow-listed set of backend RPCs over HTTP. See [web-server.md](web-server.md).

## Pages

The pages are a SvelteKit app in `ts/routes`, with shared code in `ts/lib`.
`ts/reviewer` and `ts/mathjax` are bundled separately for the sandboxed frame that
shows card content.

## Protobuf

Protocol Buffers define the backend's methods, and the storage format of some items
in a collection file. The definitions live in `proto/anki/`. `rslib/proto` generates
Rust and TypeScript from them, so pages call the backend in a type-safe way.

## Collections

A collection is the same SQLite file as in Anki, and `.apkg` and `.colpkg` files are
still read and written. So some columns, such as `usn` and the `graves` table, exist
only because they're part of the format: nothing syncs them.
