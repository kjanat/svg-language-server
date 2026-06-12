# https://just.systems

alias c := format-check
alias f := format
alias i := install-lsp
alias if := install-svg-format
alias il := install-svg-lint
alias l := lint
alias fmt := format
alias t := test
alias b := build-debug
alias br := build-release
alias cmt := commit
alias gs := generate-schemas

# list every recipe
default:
    just --list --unsorted

# rewrite tracked files to dprint style
[arg("diff", long="diff", short="d", value="--diff")]
[arg("staged", long="staged", value="--staged")]
[group('format')]
format diff="" staged="" *FILES:
    dprint fmt {{ diff }} {{ staged }} {{ FILES }}

# fail if any file needs formatting
[arg("list-different", long="list-different", short="l", value="--list-different")]
[arg("staged", long="staged", short="s", value="--staged")]
[group('format')]
format-check staged="" list-different="" *FILES:
    dprint check {{ staged }} {{ list-different }} {{ FILES }}

# install svg-language-server to cargo bin
[arg("profile", long="profile", short="p")]
[group('install')]
install-lsp profile="release":
    cargo install --path crates/svg-language-server --bin svg-language-server --profile={{ profile }}

# install svg-format CLI to cargo bin
[group('install')]
install-svg-format profile="release":
    cargo install --path crates/svg-format --bin svg-format --features="cli" --profile={{ profile }}

# install svg-lint CLI to cargo bin
[group('install')]
install-svg-lint profile="release":
    cargo install --path crates/svg-lint --bin svg-lint --features="cli" --profile={{ profile }}

# clippy the workspace; warnings are errors
[arg("allow-dirty", long="allow-dirty", short="a", value=" --allow-dirty")]
[arg("fix", long="fix", short="f", value=" --fix")]
[group('rust')]
lint fix="" allow-dirty="":
    cargo clippy --workspace --all-targets --all-features{{ fix }}{{ allow-dirty }} -- -D clippy::all

# biggest .rs files + crate bloat in the LSP binary
[group('rust')]
size-report:
    #!/usr/bin/env sh
    set -e
    cargo_bloat_missing=0
    printf '%s\n' '== Rust line counts =='
    find crates -type f -name '*.rs' -exec wc -l {} \; | sort -rn | head -20
    printf '%s\n' '== cargo-bloat =='
    if ! command -v cargo-bloat >/dev/null 2>&1; then
        cargo_bloat_missing=1
        printf '%s\n' '(missing cargo-bloat)'
    else
        CARGO_TERM_QUIET=true cargo bloat --release --crates --filter svg-language-server || cargo_bloat_missing=1
    fi
    if [ "$cargo_bloat_missing" -ne 0 ]; then
        printf '\n%s\n' "cargo-bloat is required for 'just size-report' and must run successfully. Install/update it with: cargo install cargo-bloat" >&2
        exit 1
    fi

# run every workspace test
[group('rust')]
test *ARGS:
    cargo test --workspace {{ ARGS }}

# test svg-format only (fast loop)
[group('rust')]
test-svg-format:
    cargo test -p svg-format

# test svg-lint only (fast loop)
[group('rust')]
test-svg-lint:
    cargo test -p svg-lint

# debug build, whole workspace
[group('rust')]
build-debug *ARGS:
    cargo build --workspace {{ ARGS }}

# release build, whole workspace (slow)
[group('rust')]
build-release *ARGS:
    cargo build --workspace --release {{ ARGS }}

# start the LSP over stdio
[group('rust')]
run-lsp *ARGS:
    cargo run -p svg-language-server -- {{ ARGS }}

# typecheck the Deno-checked scripts (run under Bun, type-checked by Deno)
[group('scripts')]
typecheck:
    deno task --config scripts/deno.jsonc typecheck

# run the svg-compat worker's Deno test suite
[group('scripts')]
test-deno *ARGS:
    deno task --config workers/svg-compat/deno.jsonc test {{ ARGS }}

# run every local check; stop on first failure
[group('verify')]
verify:
    just format-check
    just typecheck
    just release-config-check
    just lint
    just test
    just test-deno

# commit with an AI-written message
[arg("model", long="model", short="m")]
[arg("variant", long="variant", short="v")]
[group('git')]
commit model="openai/gpt-5.4" variant="medium" message='':
    opencode run --command commit --model={{ model }} --variant={{ variant }} '{{ message }}'

# validate cargo-dist config
[group('release')]
release-config-check:
    cargo dist plan --output-format=json > /dev/null

# preview what cargo-dist would ship
[group('release')]
release-preview *ARGS:
    cargo dist plan --allow-dirty {{ ARGS }}

# regenerate the release CI workflow
[group('release')]
release-ci-regen:
    cargo dist generate --mode=ci

# bump version, verify, commit, tag; no push
[group('release')]
release-local VERSION:
    bun scripts/release-prepare.ts {{ VERSION }}

# regenerate svg-data JSON schemas
[group('codegen')]
generate-schemas:
    cargo run -p svg-data --example generate_schemas
    dprint fmt 'crates/svg-data/**/*.json'

# check baked spec data vs live W3C + svgwg (exit 1 = stale)
[group('spec-data')]
spec-freshness *ARGS:
    cargo run -q -p svg-data --features freshness-cli --bin spec-freshness -- {{ ARGS }}

# re-vendor W3C version-history JSON (metadata only; safe) + update provenance
[group('spec-data')]
refresh-editions:
    bun scripts/refresh-editions.ts

# re-vendor svgwg sources at a new commit; pass --activate to flip the pin
[group('spec-data')]
refresh-svgwg COMMIT *FLAGS:
    bun scripts/refresh-svgwg.ts {{ COMMIT }} {{ FLAGS }}
