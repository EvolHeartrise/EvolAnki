# Testing and Coverage

Coverage is orchestrated in `justfile` with direct CLI calls, using ninja only to prepare generated build artifacts
that the test commands need.

## Quick reference

```sh
just test                        # run all tests (no coverage)
just test --coverage             # run all tests + enforce coverage thresholds
just test --coverage --html      # same + generate HTML reports under out/coverage/

just test-rust                   # Rust only
just test-rust --coverage
just test-rust --coverage --html

just test-ts                     # TypeScript/Svelte Vitest only
just test-ts --coverage
just test-ts --coverage --html
```

HTML reports are written under `out/coverage/` (gitignored).

## Coverage tools and thresholds

| Stack             | Test runner                          | Coverage tool    | Minimum |
| ----------------- | ------------------------------------ | ---------------- | ------: |
| Rust workspace    | `cargo nextest` via `cargo-llvm-cov` | `cargo-llvm-cov` |     64% |
| TypeScript/Svelte | `vitest run`                         | Vitest V8        |      5% |

## Notes

- **Rust** — `cargo-llvm-cov` is installed on demand into `out/bin/` to avoid
  polluting the global cargo install. Coverage runs rebuild the workspace with
  instrumentation, so they are slower than plain `just test-rust`.
- **TypeScript** — coverage is measured only over code reachable through
  Vitest's module graph. Svelte component rendering behavior is not covered.

## See also

- [Writing Unit Tests](unit-testing.md)
- [End-to-End Testing](e2e-testing.md)
