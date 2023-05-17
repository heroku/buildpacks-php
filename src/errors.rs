use crate::layers::bootstrap::BootstrapLayerError;
use crate::layers::composer_env::ComposerEnvLayerError;
use crate::layers::platform::PlatformLayerError;
use crate::platform::generator::PlatformGeneratorError;

#[derive(Debug)]
pub(crate) enum PhpBuildpackError {
    BootstrapLayer(BootstrapLayerError),
    PlatformLayer(PlatformLayerError),
    ComposerEnvLayer(ComposerEnvLayerError),
    Platform(PlatformGeneratorError),
}

impl From<PhpBuildpackError> for libcnb::Error<PhpBuildpackError> {
    fn from(error: PhpBuildpackError) -> Self {
        Self::BuildpackError(error)
    }
}
