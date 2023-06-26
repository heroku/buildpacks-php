use crate::utils;
use crate::{PhpBuildpack, PhpBuildpackError};
use libcnb::build::BuildContext;
use libcnb::data::buildpack::StackId;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::layer::{ExistingLayerStrategy, Layer, LayerData, LayerResult, LayerResultBuilder};
use libcnb::Buildpack;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct BootstrapLayerMetadata {
    stack: StackId,
    url: String,
    strip_path_components: usize,
    directory: PathBuf,
}

pub(crate) struct BootstrapLayer {
    pub url: String,
    pub strip_path_components: usize,
    pub directory: PathBuf,
}

impl Layer for BootstrapLayer {
    type Buildpack = PhpBuildpack;
    type Metadata = BootstrapLayerMetadata;

    fn types(&self) -> LayerTypes {
        LayerTypes {
            build: false,
            cache: false, // disabled until we start using a fixed tag for CLASSIC_BUILDPACK_VERSION
            launch: false,
        }
    }

    fn create(
        &self,
        context: &BuildContext<Self::Buildpack>,
        layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        utils::download_and_unpack_tgz_with_components_stripped_and_only_entries_under_prefix(
            &self.url,
            layer_path,
            self.strip_path_components,
            &self.directory,
        )
        .map_err(BootstrapLayerError::DownloadUnpack)?;

        let layer_metadata = generate_layer_metadata(
            &context.stack_id,
            &self.url,
            self.strip_path_components,
            &self.directory,
        );
        LayerResultBuilder::new(layer_metadata).build()
    }

    fn existing_layer_strategy(
        &self,
        context: &BuildContext<Self::Buildpack>,
        layer_data: &LayerData<Self::Metadata>,
    ) -> Result<ExistingLayerStrategy, <Self::Buildpack as Buildpack>::Error> {
        let old_metadata = &layer_data.content_metadata.metadata;
        let new_metadata = generate_layer_metadata(
            &context.stack_id,
            &self.url,
            self.strip_path_components,
            &self.directory,
        );
        if new_metadata == *old_metadata {
            Ok(ExistingLayerStrategy::Keep)
        } else {
            Ok(ExistingLayerStrategy::Recreate)
        }
    }
}

fn generate_layer_metadata(
    stack: &StackId,
    url: &str,
    strip_path_components: usize,
    directory: &Path,
) -> BootstrapLayerMetadata {
    BootstrapLayerMetadata {
        stack: stack.clone(),
        url: url.to_string(),
        strip_path_components,
        directory: directory.to_path_buf(),
    }
}

#[derive(Debug)]
pub(crate) enum BootstrapLayerError {
    DownloadUnpack(utils::DownloadUnpackError),
}

impl From<BootstrapLayerError> for PhpBuildpackError {
    fn from(error: BootstrapLayerError) -> Self {
        Self::BootstrapLayer(error)
    }
}
