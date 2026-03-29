# Releasing

## Routine release flow

1. Ensure `bun` is installed locally.
2. Run `just release-prepare <version>`.
3. Review the generated commit and local tag.
4. Push the branch and tag:
   - `git push origin <branch>`
   - `git push origin v<version>`
5. GitHub Actions builds release artifacts, creates the GitHub Release, then publishes `svg-language-server` and `svg-format`.

## npm bootstrap

The long-term path is trusted publishing from GitHub Actions using OIDC.

Because npm trusted publishers are configured per existing package, the first publish of each new package name may require a temporary `NPM_TOKEN` secret in GitHub Actions. Once the first publish exists:

1. Configure trusted publishers for:
   - `svg-language-server`
   - `svg-format`
2. Point each package at this repository and the stable workflow file `.github/workflows/release.yml`.
3. Remove the temporary `NPM_TOKEN` secret so later releases rely on OIDC only.

## Notes

- `just release-prepare <version>` updates the workspace version in `Cargo.toml`, runs local checks, creates the release commit, and creates the local `v<version>` tag. It depends on `bun` for the helper script and script typecheck.
- The custom npm publish logic lives in `.github/workflows/publish-npm-oidc.yml` and is invoked from the dist-generated release workflow.
- Do not rename `.github/workflows/release.yml` after trusted publishers are configured unless you also update npm’s trusted-publisher settings.
