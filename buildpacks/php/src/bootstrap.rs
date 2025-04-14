//! Installs shared build tooling
//!
//! > Note
//! > Docs contain implementation details which may diverge from code.
//!
//! Generates two layers and downloads code into them.
//!
//! ## PHP minimal
//!
//! This version of PHP is used to ...
//!
//! URL: <https://lang-php.s3.us-east-1.amazonaws.com/dist-heroku-24-arm64-cnb/php-min-8.3.7.tar.gz>
//!
//! This artifact is downloaded into the layer directory with contents:
//!
//! ```shell
//! $ exa --tree
//! .
//! └── bin
//!   └── php
//! ```
//!
//! This adds a version of `php` to that can be invoked for the rest of the buildpack invocation. It is
//! not exported to other buildpacks or to the app's "launch" layer.
//!
//! ## PHP classic buildpack version and "installer"
//!
//! URL: <https://github.com/heroku/heroku-buildpack-php/archive/refs/heads/cnb-installer.tar.gz>
//!
//! This file comes from the `cnb-installer` branch of the <https://github.com/heroku/heroku-buildpack-php>.
//! It represents the entirety of that branch of that repo.
//!
//! This artifact is downloaded and the env var `COMPOSER_HOME` is set to this path.
//!
//! The installed components include this "classic buildpack installer subdirectory":
//!
//! ```term
//! $ ls -1 /layers/heroku_php/bootstrap_installer/support/installer
//!
//!   README.md
//!   composer.json
//!   src
//! ```
//!
//! This path is returned as `platform_installer_path` which is explained in the
//! attached readme <https://github.com/heroku/heroku-buildpack-php/blob/cnb-installer/support/installer/README.md>
//!
//! > It then installs a minimal PHP runtime and Composer for bootstrapping. It invokes Composer to
//! > install the dependencies listed in the generated "platform.json", which, using this custom
//! > Composer Installer Plugin, will cause the installation of our builds of PHP, extensions, and
//! > programs such as the web servers - all pulled from our "platform" repository, hosted on S3.
//! > Even Composer (the right version the user's app needs) is installed a second time by this
//! > step, as well as any shared libraries that e.g. an extension needs (such as librdkafka for
//! > ext-rdkafka).
//!

use crate::layers::bootstrap::BootstrapLayer;
use crate::platform;
use crate::PhpBuildpack;
use libcnb::build::BuildContext;
use libcnb::data::layer_name;
use libcnb::layer_env::Scope;
use libcnb::Env;
use std::path::PathBuf;

const PHP_VERSION: &str = "8.3.7";
const COMPOSER_VERSION: &str = "2.7.6";
const CLASSIC_BUILDPACK_VERSION: &str = "heads/cnb-installer";
const CLASSIC_BUILDPACK_INSTALLER_SUBDIR: &str = "support/installer";

pub(crate) struct BootstrapResult {
    pub(crate) env: Env,
    pub(crate) platform_installer_path: PathBuf,
    pub(crate) classic_buildpack_path: PathBuf,
}

// TODO: Switch to libcnb's struct layer API.
#[allow(deprecated)]
pub(crate) fn bootstrap(
    context: &BuildContext<PhpBuildpack>,
) -> libcnb::Result<BootstrapResult, <PhpBuildpack as libcnb::Buildpack>::Error> {
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
