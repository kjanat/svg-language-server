# https://just.systems

alias c := check
alias f := format
alias i := install
alias if := install-format
alias l := lint
alias fmt := format
alias t := test
alias b := build
alias br := build-release
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

# Install the svg-format bin to ~/.cargo/bin
install-format profile="release":
    cargo install --path crates/svg-format --bin svg-format --profile={{ profile }}

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

# Build workspace (debug)
build *ARGS:
    cargo build --workspace {{ ARGS }}

# Build workspace (release)
build-release *ARGS:
    cargo build --workspace --release {{ ARGS }}

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
