The English strings that the Rust and TypeScript layers use, in `core/`. Each is a
generated `tr` method; see `rslib/i18n`. There's only this one language.

To drop strings that nothing uses any more:

```
export CARGO_TARGET_DIR=$PWD/out/rust
mkdir -p out/ftl-usage
cargo run -p ftl -- write-json out/ftl-usage/used.json rslib ts/routes ts/lib ts/reviewer
cargo run -p ftl -- garbage-collect out/ftl-usage ftl/core
```

Messages that another message refers to count as used. The build fails if a
`tr` call's string is gone.
