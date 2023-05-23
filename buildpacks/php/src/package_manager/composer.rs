use crate::layers::composer_cache::ComposerCacheLayer;
use crate::layers::composer_env::ComposerEnvLayer;
use crate::{utils, PhpBuildpack};
use libcnb::build::BuildContext;
use libcnb::data::layer_name;
use libcnb::layer_env::Scope;
use libcnb::Env;
use libherokubuildpack::log::log_header;
use std::process::Command;

pub(crate) fn install_dependencies(
    context: &BuildContext<PhpBuildpack>,
    command_env: &mut Env,
) -> Result<(), String> {
    dbg!(&command_env);
    // TODO: check for presence of `vendor` dir
    // TODO: validate COMPOSER_AUTH?
    let composer_cache_layer = context
        .handle_layer(layer_name!("composer_cache"), ComposerCacheLayer)
        .unwrap(); // FIXME: handle
    dbg!(&composer_cache_layer.env);

    *command_env = composer_cache_layer.env.apply(Scope::Build, command_env);
    dbg!(&command_env);

    log_header("Installing dependencies");

    utils::run_command(
        Command::new("composer")
            .current_dir(&context.app_dir)
            .envs(&*command_env)
            .args([
                "install",
                "-vv",
                "--no-dev",
                "--no-progress",
                "--no-interaction",
                "--optimize-autoloader",
                "--prefer-dist",
            ]), // .envs(
                //     &[&platform_layer.env, &composer_env_layer.env]
                //         .iter()
                //         .fold(libcnb::Env::from_current(), |final_env, layer_env| {
                //             layer_env.apply(Scope::Build, &final_env)
                //         }),
                // ),
    )
    .expect("composer install failed"); // FIXME: handle

    // this just puts the userland bin-dir on $PATH
    let composer_env_layer = context
        .handle_layer(
            layer_name!("composer_env"),
            ComposerEnvLayer {
                command_env: command_env,
                dir: &context.app_dir,
            },
        )
        .unwrap(); // FIXME: handle
    dbg!(&composer_env_layer.env);
    *command_env = composer_env_layer.env.apply(Scope::All, command_env);
    dbg!(&command_env);

    // TODO: run `composer compile`, but is that still a good name?

    Ok(())
}
