#![warn(clippy::pedantic)]

mod errors;
mod layers;
mod platform;
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

use libherokubuildpack::log::log_header;

use crate::layers::composer_env::ComposerEnvLayer;
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

        let composer_json_path = context.app_dir.join("composer.json");
        if !composer_json_path.exists() {
            fs::write(composer_json_path, "{}").expect("Failed to write empty composer.json");
        }

        log_header("Preparing platform package installation");
        let heroku_php_platform_repositories = context
            .platform
            .env()
            .get("HEROKU_PHP_PLATFORM_REPOSITORIES")
            .map_or_else(
                || {
                    Ok(vec![format!(
                        "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-stable/",
                        context.stack_id
                    )])
                },
                |urls| shell_words::split(&urls.to_string_lossy()),
            )
            .unwrap();

        let platform_json = platform::generate_platform_json(
            &context.stack_id,
            &context.app_dir,
            &bootstrap_layer.path,
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
                platform_json,
            },
        )?;

        // this just puts our platform bin-dir (with boot scripts) and the userland bin-dir on $PATH
        let composer_env_layer = context.handle_layer(
            layer_name!("composer_env"),
            ComposerEnvLayer {
                php_env: php_layer
                    .env
                    .apply(Scope::Build, &libcnb::Env::from_current()),
                php_layer_path: php_layer.path.clone(),
            },
        )?;

        log_header("Installing dependencies");
        utils::run_command(
            Command::new("composer")
                .current_dir(&context.app_dir)
                .args([
                    "install",
                    "-vv",
                    "--no-dev",
                    "--no-progress",
                    "--no-interaction",
                    "--optimize-autoloader",
                    "--prefer-dist",
                ])
                .envs(
                    &[&php_layer.env, &composer_env_layer.env]
                        .iter()
                        .fold(libcnb::Env::from_current(), |final_env, layer_env| {
                            layer_env.apply(Scope::Build, &final_env)
                        }),
                )
                .env("COMPOSER_HOME", &composer_cache_layer.path),
        )
        .expect("composer install failed");

        let default_process = ProcessBuilder::new(process_type!("web"), "heroku-php-apache2")
            .default(true)
            .direct(true)
            .build();
        BuildResultBuilder::new()
            .launch(LaunchBuilder::new().process(default_process).build())
            .build()
    }
}

buildpack_main!(PhpBuildpack);
