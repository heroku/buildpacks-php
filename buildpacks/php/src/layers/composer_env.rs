use crate::errors::PhpBuildpackError;
use crate::PhpBuildpack;
use libcnb::build::BuildContext;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::generic::GenericMetadata;
use libcnb::layer::{ExistingLayerStrategy, Layer, LayerData, LayerResult, LayerResultBuilder};
use libcnb::layer_env::{LayerEnv, ModificationBehavior, Scope};
use libcnb::{Buildpack, Env};
use libherokubuildpack::log::log_header;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) struct ComposerEnvLayer<'a> {
    pub command_env: &'a Env,
    pub dir: &'a PathBuf,
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
        &self,
        _context: &BuildContext<Self::Buildpack>,
        _layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        log_header("Preparing Composer environment");

        let output = Command::new("composer")
            .args(["config", "--no-plugins", "bin-dir"])
            .current_dir(self.dir)
            .envs(self.command_env)
            .env("PHP_INI_SCAN_DIR", "")
            .env("COMPOSER_AUTH", "")
            .output()
            .map_err(ComposerEnvLayerError::ComposerInvoke)?;

        if !output.status.success() {
            Err(ComposerEnvLayerError::ComposerBinDir(output.status.code()))?;
        }

        let composer_bin_dir: PathBuf = (*String::from_utf8_lossy(&output.stdout).trim()).into();
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

    fn existing_layer_strategy(
        &self,
        _context: &BuildContext<Self::Buildpack>,
        _layer_data: &LayerData<Self::Metadata>,
    ) -> Result<ExistingLayerStrategy, <Self::Buildpack as Buildpack>::Error> {
        Ok(ExistingLayerStrategy::Keep) // we keep this cached always, as Composer does its own cleanup from time to time
    }
}

#[derive(Debug)]
pub(crate) enum ComposerEnvLayerError {
    ComposerInvoke(std::io::Error),
    ComposerBinDir(Option<i32>),
}

impl From<ComposerEnvLayerError> for PhpBuildpackError {
    fn from(error: ComposerEnvLayerError) -> Self {
        Self::ComposerEnvLayer(error)
    }
}
