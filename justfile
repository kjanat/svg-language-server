# https://just.systems

alias c := commit
alias f := format
alias i := install
alias l := lint
alias fmt := format
alias chk := check

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
[arg("allow-dirty", long="allow-dirty", value="--allow-dirty")]
[arg("fix", long="fix", value="--fix")]
lint fix="" allow-dirty="":
    cargo clippy --workspace --all-targets --all-features {{ fix }} {{ allow-dirty }} -- -D clippy::all

# Let gippity write a nice commit message
[arg("model", long="model", short="m")]
[arg("variant", long="variant", short="v")]
commit model="openai/gpt-5.4" variant="medium" *$MESSAGE:
    opencode run --command commit --model={{ model }} --variant={{ variant }} "$MESSAGE"
