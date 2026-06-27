//! Zed extension entry point for SVG language support.
//!
//! The extension starts `svg-language-server` for SVG buffers.
//! It prefers the npm package managed by Zed and falls back to a `svg-language-server` binary
//! already available in the worktree `PATH`.
use std::{env, fs};
use zed::register_extension;
use zed_extension_api as zed;

const LSP_BINARY_NAME: &str = "svg-language-server";
const LSP_PACKAGE_NAME: &str = "svg-language-server";
const LSP_RUN_PATH: &str = "node_modules/svg-language-server/run.js";

struct SvgExtension;

impl SvgExtension {
    fn file_exists(path: &str) -> bool {
        fs::metadata(path).is_ok_and(|stat| stat.is_file())
    }

    fn install_npm_package(
        &self,
        package_name: &str,
        version: &str,
        expected_path: &str,
    ) -> zed::Result<()> {
        zed::npm_install_package(package_name, version)?;
        if !Self::file_exists(expected_path) {
            Err(format!(
                "installed package '{package_name}' did not contain expected path \
                 '{expected_path}'",
            ))?;
        }

        Ok(())
    }

    fn npm_managed_server_path() -> zed::Result<String> {
        Ok(env::current_dir()
            .map_err(|error| format!("failed to get extension working directory: {error}"))?
            .join(LSP_RUN_PATH)
            .to_string_lossy()
            .to_string())
    }

    fn npm_managed_server_script_path(
        &mut self,
        language_server_id: &zed::LanguageServerId,
    ) -> zed::Result<String> {
        let has_local_server = Self::file_exists(LSP_RUN_PATH);
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let lsp_version = match zed::npm_package_latest_version(LSP_PACKAGE_NAME) {
            Ok(version) => version,
            Err(_) if has_local_server => return Self::npm_managed_server_path(),
            Err(error) => return Err(error),
        };

        let installed_version =
            zed::npm_package_installed_version(LSP_PACKAGE_NAME).unwrap_or(None);
        if has_local_server && installed_version.as_deref() == Some(lsp_version.as_str()) {
            return Self::npm_managed_server_path();
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::Downloading,
        );

        self.install_npm_package(LSP_PACKAGE_NAME, &lsp_version, LSP_RUN_PATH)?;
        Self::npm_managed_server_path()
    }
}

impl zed::Extension for SvgExtension {
    fn new() -> Self {
        SvgExtension
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::Command> {
        match self.npm_managed_server_script_path(language_server_id) {
            Ok(server_path) => Ok(zed::Command {
                command: zed::node_binary_path()?,
                args: vec![server_path],
                env: Default::default(),
            }),
            Err(npm_error) => {
                if let Some(path) = worktree.which(LSP_BINARY_NAME) {
                    return Ok(zed::Command {
                        command: path,
                        args: vec![],
                        env: Default::default(),
                    });
                }

                Err(format!(
                    "failed to prepare npm-managed SVG tools ({npm_error}); {LSP_BINARY_NAME} not \
                     found in PATH. Install with: npm install --global svg-language-server",
                ))
            }
        }
    }
}

register_extension!(SvgExtension);
