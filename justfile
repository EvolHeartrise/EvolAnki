# Show available commands
default:
    @just --list

# Build the web pages
build:
    {{ ninja }} web

# Build and serve a collection; with no path, a demo one
run *args:
    ./tools/run-web {{ args }}

# Build and run all checks (lint + test) - lets ninja handle dependencies
check:
    {{ ninja }} check

# Run all tests (Rust, TypeScript)
test:
    {{ ninja }} check:rust_test check:vitest

# Run Rust tests
test-rust:
    {{ ninja }} check:rust_test

# Run TypeScript/Svelte Vitest tests
test-ts:
    {{ ninja }} check:vitest

# Run Playwright end-to-end tests. Pass --ui to open the interactive UI.
[arg("ui", long="ui", value="--ui")]
test-e2e ui='': _install-playwright-browsers
    {{ ninja }} web
    CARGO_TARGET_DIR=$PWD/out/rust cargo build -p evolanki-server
    {{ playwright_env }} {{ yarn }} test:e2e {{ ui }}

[private]
_install-playwright-browsers:
    {{ ninja }} node_modules
    {{ playwright_env }} {{ yarn }} playwright install chromium webkit

# Check formatting (fast, no build needed)
fmt:
    {{ ninja }} check:format

# Fix formatting
fix-fmt:
    {{ ninja }} format

# Run linting and type checking (requires build outputs)
lint:
    {{ ninja }} check:clippy check:eslint check:svelte

# Fix auto-fixable lint issues
fix-lint:
    {{ ninja }} fix:eslint

# Regenerate cargo/licenses.json and ts/licenses.json
update-licenses:
    tools/update-licenses

# Sync translation files
ftl-sync:
    {{ ninja }} ftl-sync

# Deprecate translation strings
ftl-deprecate:
    {{ ninja }} ftl-deprecate

# Build Rust API docs
docs-rust:
    cargo doc --open

# Dispatch CI workflow on a given branch or tag
ci branch:
    gh workflow run ci.yml --ref {{ branch }}

# Remove build outputs from out/ (pass keep-env to keep node_modules)
clean *args:
    ./tools/clean {{ args }}

ninja := "./ninja"
playwright_env := "PLAYWRIGHT_BROWSERS_PATH=out/playwright-browsers"
yarn := "out/extracted/node/bin/yarn"
