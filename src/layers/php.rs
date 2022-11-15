use crate::utils::{self, CommandError};
use crate::{PhpBuildpack, PhpBuildpackError};

use libcnb::build::BuildContext;
use libcnb::data::buildpack::StackId;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::layer::{Layer, LayerResult, LayerResultBuilder};
use libcnb::layer_env::{LayerEnv, ModificationBehavior, Scope};
use libcnb::{Buildpack, Env};
use libherokubuildpack::log::{log_header, log_info};
use serde::Deserialize;
use serde::Serialize;
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) struct PhpLayer {
    pub bootstrap_env: Env,
    pub composer_cache_layer_path: PathBuf,
    pub platform_json: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub(crate) struct PhpLayerMetadata {
    stack: StackId,
}

impl Layer for PhpLayer {
    type Buildpack = PhpBuildpack;
    type Metadata = PhpLayerMetadata;

    fn types(&self) -> LayerTypes {
        LayerTypes {
            build: true,
            cache: false,
            launch: true,
        }
    }

    fn create(
        &self,
        context: &BuildContext<Self::Buildpack>,
        layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        log_header("Installing platform packages");

        let mut platform_json = File::create(layer_path.join("composer.json")).unwrap();
        platform_json
            .write_all(self.platform_json.as_ref())
            .unwrap();

        utils::run_command(
            Command::new("composer")
                .current_dir(layer_path)
                .args([
                    "install",
                    "--no-dev",
                    "--no-interaction",
                    //"--no-progress",
                ])
                //.env_clear()
                .envs(&self.bootstrap_env) // we're invoking 'composer' from the bootstrap layer
                .env("COMPOSER_HOME", &self.composer_cache_layer_path), // ... but we want any caching to happen in the cache layer
        )
        .expect("platform install failed");

        // the package we just installed was built with a different './configure --prefix'
        // we're first fetching the prefix value from the php-config "program" (it's a script)
        // then we're replacing that value throughout the php-config script
        let php_config_bin = layer_path.join("bin/php-config");
        let configured_prefix = Command::new(&php_config_bin)
            .args(["--prefix"])
            .env_clear()
            //.envs(&layer_env.apply_to_empty(Scope::Build))
            .output()
            .expect("Failed to execute php-config --prefix");
        let configured_prefix = String::from_utf8_lossy(&configured_prefix.stdout);

        log_info(format!(
            "Rewriting PHP installation configure prefix from '{}' to '{}'",
            configured_prefix.trim(),
            &layer_path.to_string_lossy(),
        ));

        {
            let contents = fs::read_to_string(&php_config_bin).expect("Failed to read php-config");
            // FIXME: this is pretty blunt - should maybe not replace in --configure-options, and maybe as regex to anchor on leading non-word-non-slash character
            let new = contents.replace(configured_prefix.trim(), &layer_path.to_string_lossy());
            let mut php_config = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&php_config_bin)
                .expect("Failed to open php-config for writing");
            php_config
                .write(new.as_bytes())
                .expect("Failed to write php-config");
        } // so the file handle closes when this scope returns

        let php_ini_path = Command::new(&php_config_bin)
            .env_clear()
            .args(["--ini-path"])
            .output()
            .expect("Failed to execute php-config --ini-path");
        let php_ini_path = String::from_utf8_lossy(&php_ini_path.stdout);
        let php_ini_path = PathBuf::from(&*php_ini_path.trim()).join("php.ini");

        let extension_dir = Command::new(&php_config_bin)
            .env_clear()
            .args(["--extension-dir"])
            .output()
            .expect("Failed to execute php-config --extension-dir");
        let extension_dir = String::from_utf8_lossy(&extension_dir.stdout);

        let mut php_ini = OpenOptions::new()
            .append(true)
            .open(&php_ini_path)
            .expect("Failed to open php.ini");
        writeln!(php_ini, "extension_dir = {}", extension_dir.trim())
            .expect("Failed to modify php.ini");

        let php_ini_scan_dir = Command::new(&php_config_bin)
            .env_clear()
            .args(["--ini-dir"])
            .output()
            .expect("Failed to execute php-config --ini-dir");
        let php_ini_scan_dir = String::from_utf8_lossy(&php_ini_scan_dir.stdout);

        let layer_env = LayerEnv::new()
            .chainable_insert(Scope::All, ModificationBehavior::Delimiter, "PATH", ":")
            .chainable_insert(
                Scope::All,
                ModificationBehavior::Prepend,
                "PATH",
                &layer_path.join("sbin"),
            )
            .chainable_insert(
                Scope::All,
                ModificationBehavior::Delimiter,
                "PHP_INI_SCAN_DIR",
                ":",
            )
            .chainable_insert(
                Scope::All,
                ModificationBehavior::Prepend,
                "PHP_INI_SCAN_DIR",
                &php_ini_scan_dir.trim(),
            )
            .chainable_insert(
                Scope::All,
                ModificationBehavior::Override,
                "PHPRC",
                &php_ini_path,
            );
        let layer_metadata = generate_layer_metadata(&context.stack_id);
        LayerResultBuilder::new(layer_metadata)
            .env(layer_env)
            .build()
    }
}

fn generate_layer_metadata(stack_id: &StackId) -> PhpLayerMetadata {
    PhpLayerMetadata {
        stack: stack_id.clone(),
    }
}

#[derive(Debug)]
pub(crate) enum PhpLayerError {
    Command(CommandError),
}

impl From<PhpLayerError> for PhpBuildpackError {
    fn from(error: PhpLayerError) -> Self {
        Self::PhpLayer(error)
    }
}
