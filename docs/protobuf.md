# Protocol Buffers

The definitions in `proto/anki/` describe the backend's methods, and the storage format
of some items in a collection file. See [the architecture](architecture.md#protobuf).
For types and syntax, see the official
[language guide](https://protobuf.dev/programming-guides/proto3/).

## Services

Each `service` becomes a Rust trait in `anki::services`. One whose name starts with
`Backend`, like `BackendCollectionService`, is implemented on `Backend` and works without
an open collection; any other is implemented on `Collection`. `Backend::run_method()`
calls any method by its camelCase name, so a method name may appear in only one service.
The pages reach a method only once it's in the allow-list in
`rslib/web-server/src/rpc.rs`.

## Names

Generated code follows the naming conventions of the language: a field `foo_bar` is
`fooBar` in TypeScript, and a message `FooBar`'s namespace is `foo_bar` in Rust.

## Optional values and oneofs

Protobuf doesn't guarantee that a oneof field is set, or that an enum holds a valid
variant.

- In TypeScript, which uses [protobuf-es](https://github.com/bufbuild/protobuf-es), a
  field marked `optional` is `undefined` when unset, and a oneof is a `{ case, value }`
  object whose `case` may be `undefined`.
- In Rust, which uses [prost](https://docs.rs/prost/latest/prost/), a field marked
  `optional`, a message field and a oneof are all `Option`s. Given an enum field
  `Foo foo = 1;`, `message.foo` is an `i32`: use the accessor `message.foo()` to convert
  it. As the server's own pages send these messages, an `InvalidInput` error or
  `unwrap_or_default()` is usually fine.

`cargo doc --open -p anki_proto` shows the generated Rust.

## Compatibility

Messages that only pass between the pages and the server can be changed freely, as both
are built together.

Some messages, like `Deck`, are stored in the collection file. Don't reuse their field
numbers, or change their meaning, without a schema upgrade, as old collections must
still load, and `evolanki-admin` can read files from desktop Anki.
