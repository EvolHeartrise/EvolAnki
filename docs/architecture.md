# Architecture

EvolAnki is a local, single-user web app: a Rust server that links Anki's Rust
backend directly, and Svelte pages that talk to it.

## Backend

Nearly all logic lives in the Rust library in `rslib/`: collections, the scheduler
and FSRS, importing, and so on. Filtered decks and custom study are in
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

A collection is a SQLite file that starts out as Anki's, without what only sync used: schema
19 has no `usn` columns, `graves` table, or `scm`, `dty` and `ls` in `col`. `.apkg` and
`.colpkg` files from Anki, and older collections down to schema 11, are upgraded when opened,
and what this writes can't be read by stock Anki, which has no way back in. So an upgrade is
one way: see [web-server.md](web-server.md#schema-changes).
