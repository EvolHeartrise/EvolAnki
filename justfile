# Show available commands
default:
    @just --list

# Build the web pages
build:
    {{ ninja }} web

# Build and serve a collection; with no path, a demo one
run *args:
    ./tools/run-web {{ args }}

# Format the code, then build and run all checks (lint + test)
check:
    {{ ninja }} format
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
