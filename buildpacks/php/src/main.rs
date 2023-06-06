#![warn(clippy::pedantic)]

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

use crate::php_project::PhpProject;

pub(crate) struct PhpBuildpack;

impl Buildpack for PhpBuildpack {
    type Platform = GenericPlatform;
    type Metadata = GenericMetadata;
    type Error = PhpBuildpackError;

    fn detect(&self, context: DetectContext<Self>) -> libcnb::Result<DetectResult, Self::Error> {
        // walk over all kinds of PhpProject in our preferred order of detection, and see if one matches this codebase
        match PhpProject::detect(&context.app_dir, &context.platform.env()) {
            Some(_) => DetectResultBuilder::pass().build(),
            None => {
                log_info("No PHP project files found.");
                DetectResultBuilder::fail().build()
            }
        }
    }

    fn build(&self, context: BuildContext<Self>) -> libcnb::Result<BuildResult, Self::Error> {
        // same as in detect... get our... trait implementation? that handles this type of PHP codebase
        let mut php_project =
            PhpProject::detect(&context.app_dir, &context.platform.env()).unwrap(); // FIXME: handle

        // to bootstrap, we'll always need PHP and Composer, regardless of userland install package manager
        let bootstrap_layer = context.handle_layer(layer_name!("bootstrap"), BootstrapLayer)?;

        let mut platform_env = Env::from_current();
        dbg!(&platform_env);
        dbg!(&bootstrap_layer.env);
        // add this to our env
        platform_env = bootstrap_layer.env.apply(Scope::Build, &platform_env);
        dbg!(&platform_env);

        php_project.load(&context.app_dir).unwrap();

        log_header("Preparing platform packages installation");

        let all_repos = platform::repos_from_default_and_env(&context).unwrap();

        let (platform_json, _notices) = php_project
            .make_platform_json(
                &context.stack_id,
                &bootstrap_layer
                    .path
                    .join(layers::bootstrap::INSTALLER_SUBDIR)
                    .join("support/installer/"),
                &all_repos,
                false,
            )
            .unwrap(); // FIXME: handle

        // TODO: print notices

        let platform_cache_layer =
            context.handle_layer(layer_name!("platform_cache"), ComposerCacheLayer)?;
        dbg!(&platform_cache_layer.env);
        platform_env = platform_cache_layer.env.apply(Scope::Build, &platform_env);
        dbg!(&platform_env);

        let platform_layer = context.handle_layer(
            layer_name!("platform"),
            PlatformLayer {
                command_env: &platform_env,
                platform_json: serde_json::to_string_pretty(&platform_json).unwrap(),
            },
        )?;
        // this puts the boot scripts installed in the layer above on $PATH
        // we do this in a separate layer because
        // 1) we then do not have to merge with the package-generated env vars
        // 2) in the future, we'll have a dedicated layer for boot scripts
        context
            .handle_layer(
                layer_name!("boot_env"),
                ComposerEnvLayer {
                    command_env: &platform_env,
                    dir: &platform_layer.path,
                },
            )
            .unwrap(); // FIXME: handle
                       // no need to merge this into anything, it's for launch later

        // time for a fresh env. First, from current, so `git` is on $PATH etc
        let mut command_env = Env::from_current();
        dbg!(&command_env);
        // then we add anything from our successfull platform install
        dbg!(&platform_layer.env);
        command_env = platform_layer.env.apply(Scope::Build, &command_env);
        dbg!(&command_env);

        php_project
            .install_dependencies(&context, &mut command_env)
            .unwrap();
        dbg!(&command_env);

        let default_process = ProcessBuilder::new(process_type!("web"), vec!["heroku-php-apache2"])
            .default(true)
            .build();
        BuildResultBuilder::new()
            .launch(LaunchBuilder::new().process(default_process).build())
            .build()
    }
}

buildpack_main!(PhpBuildpack);
