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

# test all tree-sitter grammar Go bindings (workspace-wide via go.work)
[group('grammar')]
test-go *ARGS:
    go test $(go list -m | sed 's|$|/bindings/go|') {{ ARGS }}

# test all tree-sitter grammar Zig bindings (zig 0.16)
[group('grammar')]
test-zig:
    for d in svg svg-path svg-paint svg-transform; do (cd grammars/tree-sitter-$d && zig build test) || exit 1; done

# tidy all tree-sitter grammar Go modules (refresh go.mod + go.sum)
[group('grammar')]
tidy-go:
    for m in $(go list -m); do go -C "grammars/$(basename $m)" mod tidy; done

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

# regenerate spec data: fetch canonical svgwg + extract (default-branch HEAD, or pinned REF: branch/tag/SHA)
[group('spec')]
regen REF="":
    cargo run -p svg-data-regen -- {{ REF }}

# regen, printing the element/property/term named NAME as a JSON sample (optional pinned REF)
[group('spec')]
regen-sample NAME REF="":
    REGEN_SAMPLE='{{ NAME }}' cargo run -p svg-data-regen -- {{ REF }}

# run the svg-data-regen parser tests (offline, no network)
[group('spec')]
regen-test *ARGS:
    cargo test -p svg-data-regen {{ ARGS }}

# typecheck the Deno-checked scripts
[group('scripts')]
typecheck:
    deno check scripts/release-prepare.ts

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
