# https://just.systems

alias c := commit
alias f := format
alias i := install
alias l := lint
alias fmt := format

# Returns the list of all available commands
default:
    just --list --unsorted

# Format codebase
format *FILE:
    dprint fmt {{ FILE }}

# Install the lsp bin to ~/.cargo/bin
install:
    cargo install --path crates/svg-language-server --bin svg-language-server

# Clippy all
lint:
    cargo clippy --workspace --all-targets --all-features -- -D clippy::all

# Let gippity write a nice commit message
[arg("model", long="model", short="m")]
[arg("variant", long="variant", short="v")]
commit model="openai/gpt-5.4" variant="medium" *MESSAGE:
    opencode run --command commit --model={{ model }} --variant={{ variant }} '{{ MESSAGE }}'
