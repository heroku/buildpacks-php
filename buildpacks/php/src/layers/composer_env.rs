// TODO: Switch to libcnb's struct layer API.
#![allow(deprecated)]

use crate::{PhpBuildpack, PhpBuildpackError};
use bullet_stream::global::print;
use fun_run::CmdError;
use libcnb::build::BuildContext;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::generic::GenericMetadata;
use libcnb::layer::{Layer, LayerResult, LayerResultBuilder};
use libcnb::layer_env::{LayerEnv, ModificationBehavior, Scope};
use libcnb::{Buildpack, Env};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

pub(crate) struct ComposerEnvLayer<'a> {
    pub(crate) command_env: &'a Env,
    pub(crate) dir: &'a PathBuf,
}

impl Layer for ComposerEnvLayer<'_> {
    type Buildpack = PhpBuildpack;
    type Metadata = GenericMetadata;

    fn types(&self) -> LayerTypes {
        LayerTypes {
            build: true,
            cache: false,
            launch: true,
        }
    }

    fn create(
        &mut self,
        _context: &BuildContext<Self::Buildpack>,
        _layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        let output = print::sub_time_cmd(
            Command::new("composer")
                .args(["config", "--no-plugins", "bin-dir"])
                .current_dir(self.dir)
                .envs(self.command_env)
                .env("PHP_INI_SCAN_DIR", "")
                .env("COMPOSER_AUTH", ""),
        )
        .map_err(|cmd_err| match cmd_err {
            CmdError::SystemError(_, error) => ComposerEnvLayerError::ComposerInvoke(error),
            CmdError::NonZeroExitAlreadyStreamed(output)
            | CmdError::NonZeroExitNotStreamed(output) => {
                ComposerEnvLayerError::ComposerBinDir(output.status().to_owned())
            }
        })?;

        let composer_bin_dir: PathBuf = (*output.stdout_lossy().trim()).into();
        LayerResultBuilder::new(GenericMetadata::default())
            .env(
                LayerEnv::new()
                    .chainable_insert(
                        Scope::All,
                        ModificationBehavior::Append,
                        "PATH",
                        self.dir.join(composer_bin_dir),
                    )
                    .chainable_insert(Scope::All, ModificationBehavior::Delimiter, "PATH", ":"),
            )
            .build()
    }
}

#[derive(Debug)]
pub(crate) enum ComposerEnvLayerError {
    ComposerInvoke(std::io::Error),
    ComposerBinDir(ExitStatus),
}

impl From<ComposerEnvLayerError> for PhpBuildpackError {
    fn from(error: ComposerEnvLayerError) -> Self {
        Self::ComposerEnvLayer(error)
    }
}
