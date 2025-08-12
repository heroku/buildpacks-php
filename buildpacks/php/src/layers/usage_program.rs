// TODO: Switch to libcnb's struct layer API.
#![allow(deprecated)]

use crate::{PhpBuildpack, PhpBuildpackError};
use libcnb::build::BuildContext;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::generic::GenericMetadata;
use libcnb::layer::{Layer, LayerResult, LayerResultBuilder};
use libcnb::layer_env::{LayerEnv, ModificationBehavior, Scope};
use libcnb::Buildpack;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub(crate) struct UsageProgramLayer<'a> {
    pub(crate) program_name: &'a PathBuf,
}

impl Layer for UsageProgramLayer<'_> {
    type Buildpack = PhpBuildpack;
    type Metadata = GenericMetadata;

    fn types(&self) -> LayerTypes {
        LayerTypes {
            build: false,
            cache: false,
            launch: true,
        }
    }

    fn create(
        &mut self,
        _context: &BuildContext<Self::Buildpack>,
        layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        let bin_dir = layer_path.join("bin");
        fs::create_dir(layer_path.join(&bin_dir)).map_err(UsageProgramLayerError::FileWrite)?;

        let mut file = fs::File::create(bin_dir.join(self.program_name))
            .map_err(UsageProgramLayerError::FileWrite)?;
        let mut perms = file
            .metadata()
            .map_err(UsageProgramLayerError::FileWrite)?
            .permissions();
        perms.set_mode(perms.mode() | 0o111); // make executable
        file.set_permissions(perms)
            .map_err(UsageProgramLayerError::FileWrite)?;
        file.write_all(include_bytes!("../../bin/usage_program.sh"))
            .map_err(UsageProgramLayerError::FileWrite)?;

        LayerResultBuilder::new(GenericMetadata::default())
            .env(
                LayerEnv::new()
                    .chainable_insert(Scope::All, ModificationBehavior::Prepend, "PATH", bin_dir)
                    .chainable_insert(Scope::All, ModificationBehavior::Delimiter, "PATH", ":"),
            )
            .build()
    }
}

#[derive(Debug)]
pub(crate) enum UsageProgramLayerError {
    FileWrite(std::io::Error),
}

impl From<UsageProgramLayerError> for PhpBuildpackError {
    fn from(error: UsageProgramLayerError) -> Self {
        Self::UsageProgramLayer(error)
    }
}
