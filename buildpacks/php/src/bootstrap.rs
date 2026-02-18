use crate::PhpBuildpack;
use crate::layers::bootstrap::BootstrapLayer;
use crate::platform;
use libcnb::Env;
use libcnb::build::BuildContext;
use libcnb::data::layer_name;
use libcnb::layer_env::Scope;
use std::path::PathBuf;

#[rustfmt::skip]
pub(crate) const PLATFORM_REPOSITORY_SNAPSHOT: &str = "d67a9907f5a45998411f318953551f96accc3cdbb3eadbbe212e7295e53e703f";
const PHP_VERSION: &str = "8.4.18";
const COMPOSER_VERSION: &str = "2.9.3";

// TODO: Switch to libcnb's struct layer API.
#[allow(deprecated)]
pub(crate) fn bootstrap(
    context: &BuildContext<PhpBuildpack>,
) -> libcnb::Result<Env, <PhpBuildpack as libcnb::Buildpack>::Error> {
    let mut env = Env::from_current();

    let php_layer_data = context.handle_layer(
        layer_name!("bootstrap_php"),
        BootstrapLayer {
            url: platform::platform_base_url_for_target(&context.target)
                .join(&format!("php-min-{PHP_VERSION}.tar.gz"))
                .expect("Internal error: failed to generate bootstrap download URL for PHP")
                .to_string(),
            strip_path_components: 0,
            directory: PathBuf::new(),
        },
    )?;
    env = php_layer_data.env.apply(Scope::Build, &env);

    let composer_layer_data = context.handle_layer(
        layer_name!("bootstrap_composer"),
        BootstrapLayer {
            url: platform::platform_base_url_for_target(&context.target)
                .join(&format!("composer-{COMPOSER_VERSION}.tar.gz"))
                .expect("Internal error: failed to generate bootstrap download URL for Composer")
                .to_string(),
            strip_path_components: 0,
            directory: PathBuf::new(),
        },
    )?;
    env = composer_layer_data.env.apply(Scope::Build, &env);
    env.insert("COMPOSER_HOME", composer_layer_data.path);

    Ok(env)
}
