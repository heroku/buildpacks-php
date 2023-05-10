#![warn(clippy::pedantic)]

mod composer;
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
        // TODO: package_manager module with implementations for Composer and "None"
        // TODO: try having each implementation detect
        // TODO: use COMPOSER env var for detection
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
        // we assume that to bootstrap, we'll always need PHP and Composer, regardless of userland install package manager
        let bootstrap_layer = context.handle_layer(layer_name!("bootstrap"), BootstrapLayer)?;

        // TODO: move to package_manager::Composer
        // the file name is customizable
        let composer_json_name = context
            .platform
            .env()
            .get_string_lossy("COMPOSER")
            .unwrap_or("composer.json".into());
        // the lock name is the value of COMPOSER, with ".json" (if present) removed, then ".lock" added
        let composer_lock_name = format!(
            "{}.lock",
            composer_json_name
                .strip_suffix(".json") // TODO: print notice
                .unwrap_or(&composer_json_name)
        );
        let composer_json_path = context.app_dir.join(&composer_json_name);
        let composer_lock_path = context.app_dir.join(&composer_lock_name);

        // TODO: move to package_manager::None
        if !composer_json_path.exists() {
            // TODO: notice
            fs::write(&composer_json_path, "{}").expect("Failed to write empty composer.json");
            // FIXME: handle?
        }
        let composer_json = fs::read(&composer_json_path).unwrap(); // FIXME: handle
        let composer_json: composer::ComposerRootPackage =
            serde_json::from_slice(&composer_json).unwrap(); // FIXME: handle

        let composer_lock = match composer_lock_path.exists() {
            // TODO: move to package_manager::Composer
            true => serde_json::from_slice(&fs::read(&composer_lock_path).unwrap()).unwrap(),
            // TODO: move to package_manager::None
            false => composer::ComposerLock::new(Some("2.99.0".into())),
        };

        // TODO: call "composer validate"?
        // TODO: ^ yes, also for freshness check

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

        let (platform_json, notices) = composer::platform::make_platform_json(
            &composer_lock,
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

        // TODO: move to package_manager::Composer
        let composer_cache_layer =
            context.handle_layer(layer_name!("composer_cache"), ComposerCacheLayer)?;

        // TODO: rename layer to... "platform" or something?
        let php_layer = context.handle_layer(
            layer_name!("php"),
            PhpLayer {
                bootstrap_env: bootstrap_layer
                    .env
                    .apply(Scope::Build, &libcnb::Env::from_current()),
                composer_cache_layer_path: composer_cache_layer.path.clone(),
                platform_json: serde_json::to_string_pretty(&platform_json).unwrap(),
            },
        )?;

        // TODO: split up into "boot-scripts" or so layer, and later userland bin-dir layer
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

        // TODO: move to package_manger::(Composer|None), no-op in None impl
        // TODO: check for presence of `vendor` dir
        // TODO: validate COMPOSER_AUTH?
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

        // TODO: run `composer compile`, but is that still a good name?

        let default_process = ProcessBuilder::new(process_type!("web"), vec!["heroku-php-apache2"])
            .default(true)
            .build();
        BuildResultBuilder::new()
            .launch(LaunchBuilder::new().process(default_process).build())
            .build()
    }
}

buildpack_main!(PhpBuildpack);
