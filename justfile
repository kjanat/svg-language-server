# https://just.systems

# Returns the list of all available commands
default:
    just --list --unsorted

alias f := format
alias fmt := format

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
commit MODEL="openai/gpt-5.4" VARIANT="medium" *MESSAGE:
    opencode run --command 'commit' --model "{{ MODEL }}" --variant "{{ VARIANT }}" "{{ MESSAGE }}"
