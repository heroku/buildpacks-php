use crate::layers::composer_cache::ComposerCacheLayer;
use crate::layers::composer_env::ComposerEnvLayer;
use crate::layers::php::PhpLayerMetadata;
use crate::{utils, PhpBuildpack};
use libcnb::build::BuildContext;
use libcnb::data::layer_name;
use libcnb::layer::LayerData;
use libcnb::layer_env::Scope;
use libherokubuildpack::log::log_header;
use std::process::Command;

pub(crate) fn install_dependencies(
    context: &BuildContext<PhpBuildpack>,
    platform_layer: &LayerData<PhpLayerMetadata>,
) -> Result<(), String> {
    // TODO: split up into "boot-scripts" or so layer, and later userland bin-dir layer
    // this just puts our platform bin-dir (with boot scripts) and the userland bin-dir on $PATH
    let composer_env_layer = context
        .handle_layer(
            layer_name!("composer_env"),
            ComposerEnvLayer {
                php_env: platform_layer
                    .env
                    .apply(Scope::Build, &libcnb::Env::from_current()),
                php_layer_path: platform_layer.path.clone(),
            },
        )
        .unwrap(); // FIXME: handle

    // TODO: move to package_manger::(Composer|None), no-op in None impl
    // TODO: check for presence of `vendor` dir
    // TODO: validate COMPOSER_AUTH?
    let composer_cache_layer = context
        .handle_layer(layer_name!("composer_cache"), ComposerCacheLayer)
        .unwrap(); // FIXME: handle

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
                &[&platform_layer.env, &composer_env_layer.env]
                    .iter()
                    .fold(libcnb::Env::from_current(), |final_env, layer_env| {
                        layer_env.apply(Scope::Build, &final_env)
                    }),
            )
            .env("COMPOSER_HOME", &composer_cache_layer.path),
    )
    .expect("composer install failed"); // FIXME: handle

    // TODO: run `composer compile`, but is that still a good name?

    Ok(())
}
