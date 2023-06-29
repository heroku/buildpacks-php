use crate::composer::platform::{PlatformGeneratorError, PlatformGeneratorNotice};
use crate::composer::ComposerRootPackage;
use crate::layers::composer_cache::ComposerCacheLayer;
use crate::layers::composer_env::ComposerEnvLayer;
use crate::layers::php::PhpLayerMetadata;
use crate::{composer, utils, PhpBuildpack};
use libcnb::build::BuildContext;
use libcnb::data::layer_name;
use libcnb::layer::LayerData;
use libcnb::layer_env::Scope;
use libcnb::Env;
use libherokubuildpack::log::log_header;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use url::Url;

#[derive(Default)]
pub(crate) struct Composer {
    composer_json_name: String,
    composer_lock_name: String,
    composer_json: Option<crate::composer::ComposerRootPackage>,
    composer_lock: Option<crate::composer::ComposerLock>,
}

impl Composer {
    pub(crate) fn new(composer_json_name: String, composer_lock_name: String) -> Self {
        Self {
            composer_json_name,
            composer_lock_name,
            ..Default::default()
        }
    }

    pub(crate) fn attempt(project_dir: &Path, env: &Env) -> Result<Self, Vec<String>> {
        // the file name is customizable
        let composer_json_name = env
            .get_string_lossy("COMPOSER")
            .unwrap_or("composer.json".into());
        // the lock name is the value of COMPOSER, with ".json" (if present) removed, then ".lock" added
        let composer_lock_name = format!(
            "{}.lock",
            composer_json_name
                .strip_suffix(".json") // TODO: print notice
                .unwrap_or(&composer_json_name)
        );

        let r = Self::new(composer_json_name, composer_lock_name);
        if r.detect(&project_dir) {
            Ok(r)
        } else {
            Err(vec!["FIXME: what do we even say here?".into()])
        }
    }

    pub(crate) fn detect(&self, project_dir: &Path) -> bool {
        project_dir.join(&self.composer_json_name).exists()
    }

    pub(crate) fn load(&mut self, project_dir: &Path) -> Result<(), String> {
        let composer_json_path = project_dir.join(&self.composer_json_name);
        let composer_lock_path = project_dir.join(&self.composer_lock_name);

        let composer_json = fs::read(&composer_json_path).unwrap(); // FIXME: handle

        self.composer_json =
            Some(serde_json::from_slice::<composer::ComposerRootPackage>(&composer_json).unwrap()); // FIXME: handle

        self.composer_lock = match composer_lock_path.exists() {
            true => Some(serde_json::from_slice(&fs::read(&composer_lock_path).unwrap()).unwrap()),
            false => None,
        };

        Ok(())
    }

    pub(crate) fn make_platform_json(
        &self,
        stack: &str,
        installer_path: &Path,
        platform_repositories: &Vec<Url>,
        dev: bool,
    ) -> Result<(ComposerRootPackage, HashSet<PlatformGeneratorNotice>), PlatformGeneratorError>
    {
        // FIXME: assigning default first is uglybugly
        let default = composer::ComposerLock::new(Some("2.99.0".into()));
        let lock = match &self.composer_lock {
            Some(l) => l,
            None => &default,
        };

        composer::platform::make_platform_json(
            &lock,
            stack,
            installer_path,
            platform_repositories,
            dev,
        )
    }

    pub(crate) fn install_dependencies(
        &self,
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
}
