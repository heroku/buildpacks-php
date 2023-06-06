use crate::platform::generator::{
    PlatformFinalizerNotice, PlatformGeneratorError, PlatformJsonGeneratorInput,
};
use crate::{platform, PhpBuildpack};
use composer::{ComposerLock, ComposerRootPackage};
use libcnb::build::BuildContext;
use libcnb::Env;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use url::Url;

#[derive(Default)]
pub(crate) struct Composer {
    composer_json_name: String,
    composer_lock_name: String,
    composer_json: Option<ComposerRootPackage>,
    composer_lock: Option<ComposerLock>,
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
    ) -> Result<(ComposerRootPackage, HashSet<PlatformFinalizerNotice>), PlatformGeneratorError>
    {
        let (generator_input, _) = match &self.composer_lock {
            // FIXME: map notices
            Some(l) => platform::generator::extract_from_lock(l).unwrap(), // FIXME: map errors
            None => (
                PlatformJsonGeneratorInput {
                    input_name: "auto/generated".to_string(),
                    input_revision: "main".to_string(),
                    additional_require: Some(HashMap::from([(
                        "heroku-sys/composer".to_string(),
                        "*".to_string(),
                    )])),
                    ..Default::default()
                },
                HashSet::new(),
            ),
        };

        let mut ret = platform::generator::generate_platform_json(
            generator_input,
            stack,
            installer_path,
            platform_repositories,
        )?;

        let notices = platform::generator::ensure_runtime_requirement(&mut ret).unwrap(); // FIXME: map errors

        if !dev {
            // we do not want dev requirements to even get resolved, so we do not return them
            // the reason is that even with a --no-dev install, Composer has to resolve all packages, both in require and require-dev
            // but it is common for require-dev to e.g. list ext-xdebug, and if that isn't available in our repositories, even a non-dev install would fail
            ret.package.require_dev.take();
        }

        Ok((ret, notices))
    }

    pub(crate) fn install_dependencies(
        &self,
        context: &BuildContext<PhpBuildpack>,
        command_env: &mut Env,
    ) -> Result<(), String> {
        crate::package_manager::composer::install_dependencies(&context, command_env)
    }
}
