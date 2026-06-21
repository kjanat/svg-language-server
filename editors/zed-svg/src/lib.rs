use std::{env, fs};
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
        let result = zed::npm_install_package(package_name, version);
        match result {
            Ok(()) => {
                if !Self::file_exists(expected_path) {
                    Err(format!(
                        "installed package '{package_name}' did not contain expected path '{expected_path}'",
                    ))?;
                }
            }
            Err(error) => {
                if !Self::file_exists(expected_path) {
                    Err(error)?;
                }
            }
        }

        Ok(())
    }

    fn npm_managed_server_script_path(
        &mut self,
        language_server_id: &zed::LanguageServerId,
    ) -> zed::Result<String> {
        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let lsp_version = zed::npm_package_latest_version(LSP_PACKAGE_NAME)?;

        let lsp_needs_install = !Self::file_exists(LSP_RUN_PATH)
            || zed::npm_package_installed_version(LSP_PACKAGE_NAME)?.as_ref() != Some(&lsp_version);

        if lsp_needs_install {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            self.install_npm_package(LSP_PACKAGE_NAME, &lsp_version, LSP_RUN_PATH)?;
        }

        Ok(env::current_dir()
            .map_err(|error| format!("failed to get extension working directory: {error}"))?
            .join(LSP_RUN_PATH)
            .to_string_lossy()
            .to_string())
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
                    "failed to prepare npm-managed SVG tools ({npm_error}); {LSP_BINARY_NAME} not found in PATH. Install with: npm install --global svg-language-server",
                ))
            }
        }
    }
}

zed::register_extension!(SvgExtension);
