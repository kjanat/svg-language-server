# GITHUB KNOWLEDGE BASE

## OVERVIEW

Release automation only. Generated cargo-dist workflow plus one hand-maintained npm publish workflow.

## WHERE TO LOOK

| Task                        | Location                         | Notes                                        |
| --------------------------- | -------------------------------- | -------------------------------------------- |
| Change generated release CI | `../dist-workspace.toml`         | Source of truth for `workflows/release.yml`  |
| Regenerate release workflow | `../justfile`                    | Use `just dist-generate`                     |
| Change npm publish logic    | `workflows/publish-npm-oidc.yml` | Hand-maintained reusable workflow            |
| Check release process rules | `../docs/releasing.md`           | Trusted publisher + tag-driven release notes |

## CONVENTIONS

- Treat `workflows/release.yml` as generated output from `cargo-dist`.
- Keep `publish-npm-oidc.yml` aligned with artifact names emitted by cargo-dist.
- Release flow is tag-driven; local prep happens before push via `just release-prepare <version>`.
- Trusted publishing depends on the stable workflow path `.github/workflows/release.yml`.

## ANTI-PATTERNS

- Do not hand-edit `workflows/release.yml`; edit `dist-workspace.toml` and regenerate.
- Do not rename `.github/workflows/release.yml` without updating npm trusted-publisher configuration.
- Do not assume generic CI lives here; this repo’s main preflight is local `just ci`.
- Do not change artifact naming in publish logic without checking the dist manifest shape.

## NOTES

- `publish-npm-oidc.yml` prefers OIDC trusted publishing and falls back to `NPM_TOKEN` when present.
