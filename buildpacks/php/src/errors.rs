pub(crate) mod notices;

use crate::layers::bootstrap::BootstrapLayerError;
use crate::layers::composer_env::ComposerEnvLayerError;
use crate::layers::platform::PlatformLayerError;
use crate::package_manager::composer::DependencyInstallationError;
use crate::php_project::{PlatformJsonError, ProjectLoadError};
use crate::platform::{PlatformRepositoryUrlError, WebserversJsonError};

#[derive(Debug)]
pub(crate) enum PhpBuildpackError {
    ProjectLoad(ProjectLoadError),
    BootstrapLayer(BootstrapLayerError),
    PlatformLayer(PlatformLayerError),
    ComposerEnvLayer(ComposerEnvLayerError),
    PlatformJson(PlatformJsonError),
    WebserversJson(WebserversJsonError),
    PlatformRepositoryUrl(PlatformRepositoryUrlError),
    DependencyInstallation(DependencyInstallationError),
}

impl From<PhpBuildpackError> for libcnb::Error<PhpBuildpackError> {
    fn from(error: PhpBuildpackError) -> Self {
        Self::BuildpackError(error)
    }
}
