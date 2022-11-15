use crate::layers::bootstrap::BootstrapLayerError;
use crate::layers::php::PhpLayerError;
use crate::platform::PlatformGeneratorError;

#[derive(Debug)]
pub(crate) enum PhpBuildpackError {
    BootstrapLayer(BootstrapLayerError),
    PhpLayer(PhpLayerError),
    Platform(PlatformGeneratorError),
}

impl From<PhpBuildpackError> for libcnb::Error<PhpBuildpackError> {
    fn from(error: PhpBuildpackError) -> Self {
        Self::BuildpackError(error)
    }
}
