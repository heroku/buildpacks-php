#![warn(clippy::pedantic)]
#![warn(unused_crate_dependencies)]

mod bootstrap;
mod errors;
mod layers;
mod package_manager;
mod php_project;
mod platform;
#[cfg(test)]
mod tests;
mod utils;

use crate::bootstrap::BootstrapResult;
use crate::errors::notices;
use crate::layers::bootstrap::BootstrapLayerError;
use crate::layers::composer_cache::ComposerCacheLayer;
use crate::layers::composer_env::{ComposerEnvLayer, ComposerEnvLayerError};
use crate::layers::platform::{PlatformLayer, PlatformLayerError};
use crate::package_manager::composer::DependencyInstallationError;
use crate::php_project::{
    PlatformJsonError, PlatformJsonNotice, ProjectLoadError, ProjectLoaderNotice,
};
use crate::platform::{PlatformRepositoryUrlError, WebserversJsonError};
use indoc::formatdoc;
use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::data::launch::{LaunchBuilder, ProcessBuilder};
use libcnb::data::{layer_name, process_type};
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::layer_env::Scope;
use libcnb::{buildpack_main, Buildpack, Env, Platform};
use libherokubuildpack::log::{log_error, log_header, log_info};

#[cfg(test)]
use exponential_backoff as _;
#[cfg(test)]
use libcnb_test as _;

pub(crate) struct PhpBuildpack;

impl Buildpack for PhpBuildpack {
    type Platform = GenericPlatform;
    type Metadata = GenericMetadata;
    type Error = PhpBuildpackError;

    fn detect(&self, context: DetectContext<Self>) -> libcnb::Result<DetectResult, Self::Error> {
        let mut loader_notices = Vec::<ProjectLoaderNotice>::new();
        let loader = php_project::ProjectLoader::from_env(context.platform.env())
            .unwrap(&mut loader_notices);

        if loader.detect(&context.app_dir) {
            DetectResultBuilder::pass().build()
        } else {
            loader_notices
                .into_iter()
                .map(PhpBuildpackNotice::ProjectLoader)
                .for_each(notices::log);
            log_info("No PHP project files found.");
            DetectResultBuilder::fail().build()
        }
    }

    fn build(&self, context: BuildContext<Self>) -> libcnb::Result<BuildResult, Self::Error> {
        let mut loader_notices = Vec::<ProjectLoaderNotice>::new();
        let loader = php_project::ProjectLoader::from_env(context.platform.env())
            .unwrap(&mut loader_notices);
        loader_notices
            .into_iter()
            .map(PhpBuildpackNotice::ProjectLoader)
            .for_each(notices::log);

        let project = loader
            .load(&context.app_dir)
            .map_err(PhpBuildpackError::ProjectLoad)?;

        log_header("Bootstrapping");

        let BootstrapResult {
            env: mut platform_env,
            platform_installer_path,
            classic_buildpack_path,
        } = bootstrap::bootstrap(&context)?;

        let platform_cache_layer =
            context.handle_layer(layer_name!("platform_cache"), ComposerCacheLayer)?;
        platform_env = platform_cache_layer.env.apply(Scope::Build, &platform_env);

        log_header("Preparing platform packages installation");

        let all_repos = platform::platform_repository_urls_from_default_and_build_context(&context)
            .map_err(PhpBuildpackError::PlatformRepositoryUrl)?;

        let mut platform_json_notices = Vec::<PlatformJsonNotice>::new();
        let platform_json = project
            .platform_json(
                &context.stack_id,
                &platform_installer_path,
                &all_repos,
                false,
            )
            .map_err(PhpBuildpackError::PlatformJson)?
            .unwrap(&mut platform_json_notices); // Warned::unwrap() does not panic :)
        platform_json_notices
            .into_iter()
            .map(PhpBuildpackNotice::PlatformJson)
            .for_each(notices::log);

        log_header("Installing platform packages");

        let platform_layer = context.handle_layer(
            layer_name!("platform"),
            PlatformLayer {
                command_env: &platform_env,
                platform_json: &platform_json,
            },
        )?;

        log_header("Installing web servers");

        let webservers_json = platform::webservers_json(
            &context.stack_id,
            &platform_installer_path,
            &classic_buildpack_path,
            &all_repos,
        )
        .map_err(PhpBuildpackError::WebserversJson)?;

        context.handle_layer(
            layer_name!("webservers"),
            PlatformLayer {
                command_env: &platform_env,
                platform_json: &webservers_json,
            },
        )?;

        let composer_cache_layer =
            context.handle_layer(layer_name!("composer_cache"), ComposerCacheLayer)?;

        // fresh env for following command invocations - from current, so `git` is on $PATH etc
        let mut command_env = Env::from_current();
        // then we add anything from our successful platform install - PHP, Composer, etc
        command_env = platform_layer.env.apply(Scope::Build, &command_env);
        // ... and composer caching env vars
        command_env = composer_cache_layer.env.apply(Scope::Build, &command_env);

        log_header("Installing dependencies");

        package_manager::composer::install_dependencies(&context.app_dir, &command_env)
            .map_err(PhpBuildpackError::DependencyInstallation)?;

        log_header("Preparing Composer runtime environment");

        // this just puts the userland bin-dir on $PATH
        context.handle_layer(
            layer_name!("composer_env"),
            ComposerEnvLayer {
                command_env: &command_env,
                dir: &context.app_dir,
            },
        )?;

        let default_process = ProcessBuilder::new(process_type!("web"), vec!["heroku-php-apache2"])
            .default(true)
            .build();
        BuildResultBuilder::new()
            .launch(LaunchBuilder::new().process(default_process).build())
            .build()
    }

    fn on_error(&self, error: libcnb::Error<Self::Error>) {
        match error {
            libcnb::Error::BuildpackError(e) => e.on_error(),
            libcnb_error => log_error(
                "Internal buildpack error",
                formatdoc! {"
                    An unexpected internal error was reported by the framework used by this buildpack.
                    
                    {iehs}
                    
                    Details: {libcnb_error}
                ", iehs = errors::INTERNAL_ERROR_HELP_STRING},
            ),
        };
    }
}

#[derive(Debug)]
pub(crate) enum PhpBuildpackError {
    ProjectLoad(ProjectLoadError),
    BootstrapLayer(BootstrapLayerError),
    PlatformRepositoryUrl(PlatformRepositoryUrlError),
    PlatformJson(PlatformJsonError),
    WebserversJson(WebserversJsonError),
    PlatformLayer(PlatformLayerError),
    DependencyInstallation(DependencyInstallationError),
    ComposerEnvLayer(ComposerEnvLayerError),
}

#[derive(Debug)]
pub(crate) enum PhpBuildpackNotice {
    ProjectLoader(ProjectLoaderNotice),
    PlatformJson(PlatformJsonNotice),
}

buildpack_main!(PhpBuildpack);
