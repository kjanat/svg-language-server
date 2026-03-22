# https://just.systems

alias c := check
alias f := format
alias i := install
alias l := lint
alias fmt := format
alias t := test
alias b := build
alias cmt := commit

# Returns the list of all available commands
default:
    just --list --unsorted

# Format codebase
[arg("diff", long="diff", short="d", value="--diff")]
[arg("staged", long="staged", value="--staged")]
format diff="" staged="" *FILES:
    dprint fmt {{ diff }} {{ staged }} {{ FILES }}

# Check formatting
[arg("list-different", long="list-different", short="l", value="--list-different")]
[arg("staged", long="staged", short="s", value="--staged")]
check staged="" list-different="" *FILES:
    dprint check {{ staged }} {{ list-different }} {{ FILES }}

# Install the lsp bin to ~/.cargo/bin
[arg("profile", long="profile", short="p")]
install profile="release":
    cargo install --path crates/svg-language-server --bin svg-language-server --profile={{ profile }}

# Clippy all
[arg("allow-dirty", long="allow-dirty", short="a", value=" --allow-dirty")]
[arg("fix", long="fix", short="f", value=" --fix")]
lint fix="" allow-dirty="":
    cargo clippy --workspace --all-targets --all-features{{ fix }}{{ allow-dirty }} -- -D clippy::all

# Run all workspace tests
test *ARGS:
    cargo test --workspace {{ ARGS }}

# Run only svg-format tests
test-format:
    cargo test -p svg-format

# Run dprint plugin config+format tests
test-dprint-plugin:
    cargo test -p dprint-plugin-svg --test plugin_settings

# Build workspace (debug)
build *ARGS:
    cargo build --workspace {{ ARGS }}

# Build workspace (release)
build-release *ARGS:
    cargo build --workspace --release {{ ARGS }}

# Build dprint Wasm plugin binary
build-dprint-plugin:
    CFLAGS_wasm32_unknown_unknown='-DNDEBUG' cargo build -p dprint-plugin-svg --release --target wasm32-unknown-unknown

# Print dprint Wasm plugin artifact path
plugin-path:
    @if [ ! -f target/wasm32-unknown-unknown/release/dprint_plugin_svg.wasm ]; then just build-dprint-plugin; fi
    @if [ ! -f target/wasm32-unknown-unknown/release/dprint_plugin_svg.wasm ]; then echo "Plugin artifact not found after build." >&2; exit 1; fi
    @echo "$(pwd)/target/wasm32-unknown-unknown/release/dprint_plugin_svg.wasm"

# Local preflight checks
ci:
    just check
    just lint
    just test

# Run LSP server locally
run-lsp *ARGS:
    cargo run -p svg-language-server -- {{ ARGS }}

# Let gippity write a nice commit message
[arg("model", long="model", short="m")]
[arg("variant", long="variant", short="v")]
commit model="openai/gpt-5.4" variant="medium" *$MESSAGE:
    opencode run --command commit --model={{ model }} --variant={{ variant }} "$MESSAGE"
