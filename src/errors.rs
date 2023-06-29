use crate::composer::platform::PlatformGeneratorError;
use crate::layers::bootstrap::BootstrapLayerError;
use crate::layers::composer_env::ComposerEnvLayerError;
use crate::layers::php::PhpLayerError;

#[derive(Debug)]
pub(crate) enum PhpBuildpackError {
    BootstrapLayer(BootstrapLayerError),
    PhpLayer(PhpLayerError),
    ComposerEnvLayer(ComposerEnvLayerError),
    Platform(PlatformGeneratorError),
}

impl From<PhpBuildpackError> for libcnb::Error<PhpBuildpackError> {
    fn from(error: PhpBuildpackError) -> Self {
        Self::BuildpackError(error)
    }
}
