use crate::utils::{self, DownloadUnpackError};
use crate::{PhpBuildpack, PhpBuildpackError};

use libcnb::build::BuildContext;
use libcnb::data::buildpack::StackId;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::layer::{ExistingLayerStrategy, Layer, LayerData, LayerResult, LayerResultBuilder};
use libcnb::layer_env::LayerEnv;
use libcnb::Buildpack;
use libherokubuildpack::log;
use serde::{Deserialize, Serialize};
use std::path::Path;

const PHP_VERSION: &str = "8.1.12";
const COMPOSER_VERSION: &str = "2.4.4";
const INSTALLER_VERSION: &str = "heads/cnb-installer";
pub(crate) const INSTALLER_SUBDIR: &str = "heroku-buildpack-php-cnb-installer";

pub(crate) struct BootstrapLayer;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub(crate) struct BootstrapLayerMetadata {
    stack: StackId,
    php_version: String,
    composer_version: String,
    installer_version: String,
}

impl Layer for BootstrapLayer {
    type Buildpack = PhpBuildpack;
    type Metadata = BootstrapLayerMetadata;

    fn types(&self) -> LayerTypes {
        LayerTypes {
            build: false,
            cache: false, // disabled until installer "archive" is stable
            launch: false,
        }
    }

    fn create(
        &self,
        context: &BuildContext<Self::Buildpack>,
        layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        log::log_header("Bootstrapping");

        let php_min_archive_url = format!(
            "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-stable/php-min-{}.tar.gz",
            context.stack_id, PHP_VERSION
        );
        let composer_archive_url = format!(
            "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-stable/composer-{}.tar.gz",
            context.stack_id, COMPOSER_VERSION
        );
        let installer_archive_url = format!(
            "https://github.com/heroku/heroku-buildpack-php/archive/refs/{INSTALLER_VERSION}.tar.gz"
        );

        utils::download_and_unpack_gzip(&php_min_archive_url, layer_path)
            .map_err(BootstrapLayerError::DownloadUnpack)?;
        utils::download_and_unpack_gzip(&composer_archive_url, layer_path)
            .map_err(BootstrapLayerError::DownloadUnpack)?;
        utils::download_and_unpack_gzip(&installer_archive_url, layer_path)
            .map_err(BootstrapLayerError::DownloadUnpack)?;

        let layer_metadata = generate_layer_metadata(&context.stack_id);
        LayerResultBuilder::new(layer_metadata)
            .env(LayerEnv::new())
            .build()
    }

    fn existing_layer_strategy(
        &self,
        context: &BuildContext<Self::Buildpack>,
        layer_data: &LayerData<Self::Metadata>,
    ) -> Result<ExistingLayerStrategy, <Self::Buildpack as Buildpack>::Error> {
        let old_metadata = &layer_data.content_metadata.metadata;
        let new_metadata = generate_layer_metadata(&context.stack_id);
        if new_metadata == *old_metadata {
            log::log_header("Bootstrapping (from cache)");
            Ok(ExistingLayerStrategy::Keep)
        } else {
            Ok(ExistingLayerStrategy::Recreate)
        }
    }
}

fn generate_layer_metadata(stack_id: &StackId) -> BootstrapLayerMetadata {
    BootstrapLayerMetadata {
        stack: stack_id.clone(),
        php_version: PHP_VERSION.to_string(),
        composer_version: COMPOSER_VERSION.to_string(),
        installer_version: INSTALLER_VERSION.to_string(),
    }
}

#[derive(Debug)]
pub(crate) enum BootstrapLayerError {
    DownloadUnpack(DownloadUnpackError),
}

impl From<BootstrapLayerError> for PhpBuildpackError {
    fn from(error: BootstrapLayerError) -> Self {
        Self::BootstrapLayer(error)
    }
}
