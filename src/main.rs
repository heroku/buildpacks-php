#![warn(clippy::pedantic)]

mod errors;
mod layers;
mod platform;
mod utils;

use crate::errors::PhpBuildpackError;
use crate::layers::bootstrap::BootstrapLayer;
use crate::layers::composer_cache::ComposerCacheLayer;
use crate::layers::php::PhpLayer;

use libcnb::build::{BuildContext, BuildResult, BuildResultBuilder};
use libcnb::data::build_plan::BuildPlanBuilder;
use libcnb::data::launch::{LaunchBuilder, ProcessBuilder};
use libcnb::data::{layer_name, process_type};
use libcnb::detect::{DetectContext, DetectResult, DetectResultBuilder};
use libcnb::generic::{GenericMetadata, GenericPlatform};
use libcnb::layer_env::Scope;
use libcnb::{buildpack_main, Buildpack, Platform};

use libherokubuildpack::log::log_header;

use shell_words;

use std::process::Command;

pub(crate) struct PhpBuildpack;

impl Buildpack for PhpBuildpack {
    type Platform = GenericPlatform;
    type Metadata = GenericMetadata;
    type Error = PhpBuildpackError;

    fn detect(&self, _context: DetectContext<Self>) -> libcnb::Result<DetectResult, Self::Error> {
        DetectResultBuilder::pass()
            .build_plan(
                BuildPlanBuilder::new()
                    .provides("php")
                    .requires("php")
                    .build(),
            )
            .build()
    }

    fn build(&self, context: BuildContext<Self>) -> libcnb::Result<BuildResult, Self::Error> {
        let bootstrap_layer = context.handle_layer(layer_name!("bootstrap"), BootstrapLayer)?;

        if !context.app_dir.join("composer.json").exists() {
            // TODO: write empty JSON
        }

        log_header("Preparing platform package installation");
        let heroku_php_platform_repositories = context
            .platform
            .env()
            .get("HEROKU_PHP_PLATFORM_REPOSITORIES")
            .map_or_else(
                || {
                    Ok(vec![String::from(format!(
                        "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-stable/",
                        context.stack_id
                    ))])
                },
                |urls| shell_words::split(&urls.to_string_lossy()),
            )
            .unwrap();

        let platform_json = platform::generate_platform_json(
            &context.stack_id,
            &context.app_dir,
            &bootstrap_layer.path.clone(),
            &bootstrap_layer.env.apply_to_empty(Scope::Build),
            heroku_php_platform_repositories,
        )
        .map_err(PhpBuildpackError::Platform)?;

        let composer_cache_layer =
            context.handle_layer(layer_name!("composer_cache"), ComposerCacheLayer)?;

        let php_layer = context.handle_layer(
            layer_name!("php"),
            PhpLayer {
                bootstrap_env: bootstrap_layer
                    .env
                    .apply(Scope::Build, &libcnb::Env::from_current()),
                composer_cache_layer_path: composer_cache_layer.path.clone(),
                platform_json: platform_json,
            },
        )?;

        log_header("Installing dependencies");
        utils::run_command(
            Command::new("composer")
                .current_dir(context.app_dir)
                .args([
                    "install",
                    "-vv",
                    "--no-dev",
                    "--no-progress",
                    "--no-interaction",
                    "--optimize-autoloader",
                    "--prefer-dist",
                ])
                //.env_clear()
                .envs(
                    &php_layer
                        .env
                        .apply(Scope::Build, &libcnb::Env::from_current()), // TODO: is this right? we want "system" $PATH, to access "unzip"
                )
                .env("COMPOSER_HOME", &composer_cache_layer.path),
        )
        .expect("composer install failed");

        let default_process = ProcessBuilder::new(process_type!("web"), "php")
            .args(["-S", "0.0.0.0:$PORT"])
            .default(true)
            .build();
        BuildResultBuilder::new()
            .launch(LaunchBuilder::new().process(default_process).build())
            .build()
    }
}

buildpack_main!(PhpBuildpack);
