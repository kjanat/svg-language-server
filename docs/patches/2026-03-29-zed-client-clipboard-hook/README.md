This directory preserves the experimental `editor.copyToClipboard` path that
depends on Zed core changes.

Files:

- `svg-language-server.diff`: LSP-side hook experiment.
- `zed-editor.diff`: Zed core interception of a client clipboard command.
- `zed-svg.diff`: `zed-svg` initialization-options opt-in for the hook.

Notes:

- The LSP patch intentionally omits the binary `Cargo.lock` diff.
- These patches are archival only; the active implementation no longer depends
  on them.
