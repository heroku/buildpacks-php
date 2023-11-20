use crate::layers::bootstrap::BootstrapLayer;
use crate::PhpBuildpack;
use libcnb::build::BuildContext;
use libcnb::data::layer_name;
use libcnb::layer_env::Scope;
use libcnb::Env;
use std::path::PathBuf;

const PHP_VERSION: &str = "8.1.12";
const COMPOSER_VERSION: &str = "2.4.4";
const CLASSIC_BUILDPACK_VERSION: &str = "heads/cnb-installer";
const CLASSIC_BUILDPACK_INSTALLER_SUBDIR: &str = "support/installer";

pub(crate) struct BootstrapResult {
    pub(crate) env: Env,
    pub(crate) platform_installer_path: PathBuf,
    pub(crate) classic_buildpack_path: PathBuf,
}

pub(crate) fn bootstrap(
    context: &BuildContext<PhpBuildpack>,
) -> libcnb::Result<BootstrapResult, <PhpBuildpack as libcnb::Buildpack>::Error> {
    let mut env = Env::from_current();

    let php_layer_data = context.handle_layer(
        layer_name!("bootstrap_php"),
        BootstrapLayer {
            url: format!(
                "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-stable/php-min-{}.tar.gz",
                context.stack_id, PHP_VERSION
            ),
            strip_path_components: 0,
            directory: PathBuf::new(),
        },
    )?;
    env = php_layer_data.env.apply(Scope::Build, &env);

    let composer_layer_data = context.handle_layer(
        layer_name!("bootstrap_composer"),
        BootstrapLayer {
            url: format!(
                "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-stable/composer-{}.tar.gz",
                context.stack_id, COMPOSER_VERSION
            ),
            strip_path_components: 0,
            directory: PathBuf::new(),
        },
    )?;
    env = composer_layer_data.env.apply(Scope::Build, &env);
    env.insert("COMPOSER_HOME", composer_layer_data.path);

    let classic_buildpack_layer_data = context.handle_layer(
        layer_name!("bootstrap_installer"),
        BootstrapLayer {
            url: format!(
                "https://github.com/heroku/heroku-buildpack-php/archive/refs/{CLASSIC_BUILDPACK_VERSION}.tar.gz",
            ),
            strip_path_components: 1,
            directory: PathBuf::new(),
        },
    )?;

    Ok(BootstrapResult {
        env,
        platform_installer_path: classic_buildpack_layer_data
            .path
            .join(CLASSIC_BUILDPACK_INSTALLER_SUBDIR),
        classic_buildpack_path: classic_buildpack_layer_data.path,
    })
}
