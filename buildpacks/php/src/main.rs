#![warn(clippy::pedantic)]
#![warn(unused_crate_dependencies)]

mod errors;
mod layers;
mod package_manager;
mod php_project;
mod platform;
mod utils;

use crate::errors::PhpBuildpackError;
use crate::layers::bootstrap::BootstrapLayer;
use crate::layers::composer_cache::ComposerCacheLayer;
use crate::layers::platform::PlatformLayer;

use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::data::launch::{LaunchBuilder, ProcessBuilder};
use libcnb::data::{layer_name, process_type};
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::layer_env::Scope;
use libcnb::{buildpack_main, Buildpack, Env, Platform};

use crate::layers::composer_env::ComposerEnvLayer;
use libherokubuildpack::log::{log_header, log_info};

pub(crate) struct PhpBuildpack;

impl Buildpack for PhpBuildpack {
    type Platform = GenericPlatform;
    type Metadata = GenericMetadata;
    type Error = PhpBuildpackError;

    fn detect(&self, context: DetectContext<Self>) -> libcnb::Result<DetectResult, Self::Error> {
        // walk over all kinds of PhpProject in our preferred order of detection, and see if one matches this codebase
        if php_project::ProjectLoader::from_env(&context.platform.env()).detect(&context.app_dir) {
            DetectResultBuilder::pass().build()
        } else {
            log_info("No PHP project files found.");
            DetectResultBuilder::fail().build()
        }
    }

    fn build(&self, context: BuildContext<Self>) -> libcnb::Result<BuildResult, Self::Error> {
        let project = php_project::ProjectLoader::from_env(&context.platform.env())
            .load(&context.app_dir)
            .map_err(PhpBuildpackError::ProjectLoad)?;

        // to bootstrap, we'll need PHP and Composer
        let bootstrap_layer = context.handle_layer(layer_name!("bootstrap"), BootstrapLayer)?;
        // dbg!(&bootstrap_layer.env);

        let mut platform_env = Env::from_current();
        // dbg!(&platform_env);
        // add this to our env
        platform_env = bootstrap_layer.env.apply(Scope::Build, &platform_env);
        // dbg!(&platform_env);

        log_header("Preparing platform packages installation");

        let all_repos = platform::platform_repository_urls_from_default_and_build_context(&context)
            .map_err(PhpBuildpackError::PlatformRepositoryUrl)?;

        let (platform_json, _platform_json_notices) = project
            .platform_json(
                &context.stack_id,
                &bootstrap_layer
                    .path
                    .join(layers::bootstrap::INSTALLER_SUBDIR)
                    .join("support/installer/"),
                &all_repos,
                false,
            )
            .map_err(PhpBuildpackError::PlatformJson)?;
        // TODO: print notices

        let platform_cache_layer =
            context.handle_layer(layer_name!("platform_cache"), ComposerCacheLayer)?;
        // dbg!(&platform_cache_layer.env);
        platform_env = platform_cache_layer.env.apply(Scope::Build, &platform_env);
        // dbg!(&platform_env);

        let platform_layer = context.handle_layer(
            layer_name!("platform"),
            PlatformLayer {
                command_env: &platform_env,
                platform_json: &platform_json,
            },
        )?;
        // this puts the boot scripts installed in the layer above on $PATH
        // we do this in a separate layer because
        // 1) we then do not have to merge with the package-generated env vars
        // 2) in the future, we'll have a dedicated layer for boot scripts
        context.handle_layer(
            layer_name!("boot_env"),
            ComposerEnvLayer {
                command_env: &platform_env,
                dir: &platform_layer.path,
            },
        )?;
        // no need to merge this layer's env into anything, it's for launch later

        // time for a fresh env. First, from current, so `git` is on $PATH etc
        let mut command_env = Env::from_current();
        // dbg!(&command_env);
        // then we add anything from our successful platform install
        // dbg!(&platform_layer.env);
        command_env = platform_layer.env.apply(Scope::Build, &command_env);
        // dbg!(&command_env);

        let composer_cache_layer =
            context.handle_layer(layer_name!("composer_cache"), ComposerCacheLayer)?;
        // dbg!(&composer_cache_layer.env);
        command_env = composer_cache_layer.env.apply(Scope::Build, &command_env);
        // dbg!(&command_env);

        log_header("Installing dependencies");

        project
            .install_dependencies(&context, &mut command_env)
            .map_err(PhpBuildpackError::DependencyInstallation)?;

        // this just puts the userland bin-dir on $PATH
        let composer_env_layer = context.handle_layer(
            layer_name!("composer_env"),
            ComposerEnvLayer {
                command_env: &command_env,
                dir: &context.app_dir,
            },
        )?;
        // dbg!(&composer_env_layer.env);
        command_env = composer_env_layer.env.apply(Scope::All, &command_env);
        // dbg!(&command_env);

        let default_process = ProcessBuilder::new(process_type!("web"), vec!["heroku-php-apache2"])
            .default(true)
            .build();
        BuildResultBuilder::new()
            .launch(LaunchBuilder::new().process(default_process).build())
            .build()
    }
}

buildpack_main!(PhpBuildpack);
