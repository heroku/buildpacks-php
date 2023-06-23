#![warn(clippy::pedantic)]
#![warn(unused_crate_dependencies)]

mod errors;
mod layers;
mod package_manager;
mod php_project;
mod platform;
#[cfg(test)]
mod tests;
mod utils;

use crate::errors::PhpBuildpackError;
use crate::layers::bootstrap::BootstrapLayer;
use crate::layers::composer_cache::ComposerCacheLayer;
use crate::layers::composer_env::ComposerEnvLayer;
use crate::layers::platform::PlatformLayer;
use crate::php_project::{PlatformJsonNotice, ProjectLoaderNotice};
use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::data::launch::{LaunchBuilder, ProcessBuilder};
use libcnb::data::{layer_name, process_type};
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::layer_env::Scope;
use libcnb::{buildpack_main, Buildpack, Env, Platform};
use libherokubuildpack::log::{log_header, log_info};

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
            // TODO: print notices
            log_info("No PHP project files found.");
            DetectResultBuilder::fail().build()
        }
    }

    fn build(&self, context: BuildContext<Self>) -> libcnb::Result<BuildResult, Self::Error> {
        let mut loader_notices = Vec::<ProjectLoaderNotice>::new();
        let loader = php_project::ProjectLoader::from_env(context.platform.env())
            .unwrap(&mut loader_notices);
        // TODO: print notices

        let project = loader
            .load(&context.app_dir)
            .map_err(PhpBuildpackError::ProjectLoad)?;

        // to install platform packages, we'll need PHP and Composer, as well as the Composer Installer Plugin from the classic buildpack
        let bootstrap_layer = context.handle_layer(layer_name!("bootstrap"), BootstrapLayer)?;
        let platform_installer_path = &bootstrap_layer
            .path
            .join(layers::bootstrap::CLASSIC_BUILDPACK_SUBDIR)
            .join(layers::bootstrap::CLASSIC_BUILDPACK_INSTALLER_SUBDIR);

        log_header("Preparing platform packages installation");

        let all_repos = platform::platform_repository_urls_from_default_and_build_context(&context)
            .map_err(PhpBuildpackError::PlatformRepositoryUrl)?;

        let mut platform_json_notices = Vec::<PlatformJsonNotice>::new();
        let platform_json = project
            .platform_json(
                &context.stack_id,
                platform_installer_path,
                &all_repos,
                false,
            )
            .map_err(PhpBuildpackError::PlatformJson)?
            .unwrap(&mut platform_json_notices); // Warned::unwrap() does not panic :)
                                                 // TODO: print notices

        let platform_cache_layer =
            context.handle_layer(layer_name!("platform_cache"), ComposerCacheLayer)?;

        // env to execute bootstrapping steps - from current, so `git` is on $PATH etc
        let mut platform_env = Env::from_current();
        // plus bootstrapped PHP/Composer/installer...
        platform_env = bootstrap_layer.env.apply(Scope::Build, &platform_env);
        // ... and a cache
        platform_env = platform_cache_layer.env.apply(Scope::Build, &platform_env);

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
            platform_installer_path,
            &bootstrap_layer
                .path
                .join(layers::bootstrap::CLASSIC_BUILDPACK_SUBDIR),
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

        project
            .install_dependencies(&context.app_dir, &mut command_env)
            .map_err(PhpBuildpackError::DependencyInstallation)?;

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
}

buildpack_main!(PhpBuildpack);
