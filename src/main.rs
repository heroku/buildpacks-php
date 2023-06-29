#![warn(clippy::pedantic)]

mod composer;
mod errors;
mod layers;
mod package_manager;
mod php_project;
mod utils;

use crate::errors::PhpBuildpackError;
use crate::layers::bootstrap::BootstrapLayer;
use crate::layers::composer_cache::ComposerCacheLayer;
use crate::layers::php::PhpLayer;
use std::fs;

use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::data::build_plan::BuildPlanBuilder;
use libcnb::data::launch::{LaunchBuilder, ProcessBuilder};
use libcnb::data::{layer_name, process_type};
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::layer_env::Scope;
use libcnb::{buildpack_main, Buildpack, Platform};

use libherokubuildpack::log::{log_header, log_info};

use crate::layers::composer_env::ComposerEnvLayer;
use crate::php_project::PhpProject;
use std::process::Command;

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

        // TODO: lint userland composer.json

        // TODO: enforce presence of userland composer.lock if composer.json lists requires

        // TODO: call "composer validate"?
        //       ^ yes, also for lockfile freshness check
        //       ^ also as a fallback validation for when we have a Category::Data error

        // TODO: validate composer.lock

        php_project.load(&context.app_dir).unwrap();

        // FIXME: we have to fail (or warn?) if heroku/heroku-buildpack-php is a dependency

        log_header("Preparing platform packages installation");

        // our default repo
        let default_platform_repositories = vec![url::Url::parse(
            format!(
                "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-cnb/",
                context.stack_id,
            )
            .as_str(),
        )
        .expect("Internal error: failed to parse default repository URL")];

        // anything user-supplied
        let user_repos = context
            .platform
            .env()
            .get_string_lossy("HEROKU_PHP_PLATFORM_REPOSITORIES")
            .unwrap_or("".into());

        let all_repos = composer::platform::repos_from_defaults_and_list(
            &default_platform_repositories,
            &user_repos,
        )
        .unwrap(); // FIXME: handle
                   // TODO: message if default disabled?
                   // TODO: message for additional repos?

        let (platform_json, notices) = php_project
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

        // TODO: rename layer to... "platform" or something?
        let php_layer = context.handle_layer(
            layer_name!("php"),
            PhpLayer {
                bootstrap_env: bootstrap_layer
                    .env
                    .apply(Scope::Build, &libcnb::Env::from_current()),
                composer_cache_layer_path: platform_cache_layer.path.clone(),
                platform_json: serde_json::to_string_pretty(&platform_json).unwrap(),
            },
        )?;

        php_project
            .install_dependencies(&context, &php_layer)
            .unwrap();

        let default_process = ProcessBuilder::new(process_type!("web"), vec!["heroku-php-apache2"])
            .default(true)
            .build();
        BuildResultBuilder::new()
            .launch(LaunchBuilder::new().process(default_process).build())
            .build()
    }
}

buildpack_main!(PhpBuildpack);
