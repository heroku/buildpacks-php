//! Creates a cache dir that composer can use
//!
//! Creates a directory and defines the `COMPOSER_CACHE_DIR` env var which composer
//! uses (docs: <https://getcomposer.org/doc/03-cli.md#composer-cache-dir>).
//!
//! From <https://getcomposer.org/doc/06-config.md#cache-files-ttl>
//!
//! > Composer caches all dist (zip, tar, ...) packages that it downloads. Those are purged after
//! > six months of being unused by default.

// TODO: Switch to libcnb's struct layer API.
#![allow(deprecated)]

use crate::PhpBuildpack;
use libcnb::build::BuildContext;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::generic::GenericMetadata;
use libcnb::layer::{ExistingLayerStrategy, Layer, LayerData, LayerResult, LayerResultBuilder};
use libcnb::layer_env::{LayerEnv, ModificationBehavior, Scope};
use libcnb::Buildpack;
use std::path::Path;

pub(crate) struct ComposerCacheLayer;

impl Layer for ComposerCacheLayer {
    type Buildpack = PhpBuildpack;
    type Metadata = GenericMetadata;

    fn types(&self) -> LayerTypes {
        LayerTypes {
            build: false,
            cache: true,
            launch: false,
        }
    }

    fn create(
        &mut self,
        _context: &BuildContext<Self::Buildpack>,
        layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        LayerResultBuilder::new(GenericMetadata::default())
            .env(LayerEnv::new().chainable_insert(
                Scope::Build,
                ModificationBehavior::Override,
                "COMPOSER_CACHE_DIR",
                layer_path,
            ))
            .build()
    }

    fn existing_layer_strategy(
        &mut self,
        _context: &BuildContext<Self::Buildpack>,
        _layer_data: &LayerData<Self::Metadata>,
    ) -> Result<ExistingLayerStrategy, <Self::Buildpack as Buildpack>::Error> {
        Ok(ExistingLayerStrategy::Keep) // we keep this cached always, as Composer does its own cleanup from time to time
    }
}
