set shell := ["bash", "-euo", "pipefail", "-c"]
set positional-arguments

# Yarn comes from Node's corepack, at the version in package.json.
yarn := "corepack yarn"
export COREPACK_ENABLE_DOWNLOAD_PROMPT := "0"

# Show available commands
default:
    @just --list

# Build the web pages into out/web, the folder the server serves
build: gen
    rm -rf out/web
    {{ yarn }} build
    node ts/esbuild.mjs ts/reviewer/index_wrapper.ts out/web/js/reviewer.js bundle
    node ts/esbuild.mjs ts/mathjax/index.ts out/web/js/mathjax.js transform
    mkdir -p out/web/css out/web/imgs out/web/js/vendor/mathjax
    node_modules/.bin/sass --style=compressed --no-source-map --load-path=. --load-path=node_modules \
        ts/reviewer/reviewer.scss:out/web/css/reviewer.css ts/lib/sass/card.scss:out/web/css/card.css
    cp ts/icons/favicon.ico out/web/imgs/
    cd node_modules/mathjax/es5 && cp --parents tex-chtml-full.js \
        a11y/{assistive-mml,complexity,explorer,semantic-enrich}.js output/chtml/fonts/woff-v2/*.woff \
        sre/mathmaps/{de,en,es,fr,hi,it,nemeth}.json ../../../out/web/js/vendor/mathjax/

# Build and serve a collection; with no path, a demo one
run *args:
    ./tools/run-web "$@"

# Format the code, then build and run all checks (lint + test)
check: fix-fmt build lint test

# Run all tests (Rust, TypeScript)
test: test-rust test-ts

# Run Rust tests
test-rust:
    cargo nextest --version >/dev/null 2>&1 || cargo install --locked cargo-nextest@0.9.99
    cargo nextest run --locked --failure-output=final --status-level=none

# Run TypeScript/Svelte Vitest tests
test-ts: gen
    {{ yarn }} vitest:once

# Check formatting (fast, no build needed)
fmt: deps
    cd cargo/format && cargo fmt --all --check
    node_modules/.bin/dprint check
    node_modules/.bin/prettier --cache --check "**/*.svelte"
    node_modules/.bin/tsx ts/tools/sql_format.ts check $(find rslib -name '*.sql')

# Fix formatting
fix-fmt: deps
    cd cargo/format && cargo fmt --all
    node_modules/.bin/dprint fmt
    node_modules/.bin/prettier --cache --write "**/*.svelte"
    node_modules/.bin/tsx ts/tools/sql_format.ts fix $(find rslib -name '*.sql')

# Run linting and type checking. Not alongside a build: both write ts/.svelte-kit.
lint: gen
    cargo clippy --locked --tests -- -Dclippy::dbg_macro -Dwarnings
    node_modules/.bin/eslint --max-warnings=0 -c .eslintrc.cjs ts
    {{ yarn }} svelte-check:once

# Fix auto-fixable lint issues
fix-lint: deps
    node_modules/.bin/eslint --max-warnings=0 -c .eslintrc.cjs --fix ts

# Generate the protobuf code for the pages, into out/ts/lib/generated
gen: deps
    cargo build --locked -p anki_proto
    rm -rf out/ts/lib/generated/anki
    node ts/tools/genproto.mjs out/rslib/proto/descriptors.bin out/ts/lib/generated
    cp ts/lib/generated/*.ts out/ts/lib/generated/

# Install the JavaScript dependencies, when package.json or yarn.lock change
deps:
    # node_modules used to be a link into out/
    if [ -L node_modules ]; then rm node_modules; fi
    if [ ! node_modules/.installed -nt yarn.lock ] || [ ! node_modules/.installed -nt package.json ]; then \
        {{ yarn }} install --immutable && touch node_modules/.installed; fi

# Regenerate cargo/licenses.json and ts/licenses.json
update-licenses: deps
    tools/update-licenses

# Build Rust API docs
docs-rust:
    cargo doc --open

# Dispatch CI workflow on a given branch or tag
ci branch:
    gh workflow run ci.yml --ref "$1"

# Remove build outputs from out/ (pass keep-env to keep node_modules)
clean *args:
    rm -rf out ts/.svelte-kit
    if [ "${1:-}" != keep-env ]; then rm -rf node_modules; fi
