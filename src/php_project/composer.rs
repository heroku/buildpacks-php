use crate::composer::ComposerRootPackage;
use crate::layers::php::PhpLayerMetadata;
use crate::platform::generator::{PlatformGeneratorError, PlatformGeneratorNotice};
use crate::{composer, platform, PhpBuildpack};
use libcnb::build::BuildContext;
use libcnb::layer::LayerData;
use libcnb::Env;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use url::Url;

#[derive(Default)]
pub(crate) struct Composer {
    composer_json_name: String,
    composer_lock_name: String,
    composer_json: Option<ComposerRootPackage>,
    composer_lock: Option<composer::ComposerLock>,
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
            Some(serde_json::from_slice::<ComposerRootPackage>(&composer_json).unwrap()); // FIXME: handle

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

        platform::generator::generate_platform_json(
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
        crate::package_manager::composer::install_dependencies(&context, &platform_layer)
    }
}
