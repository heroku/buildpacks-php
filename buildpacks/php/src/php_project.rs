use crate::package_manager::composer::{
    DependencyInstallationError, PlatformExtractorError, PlatformExtractorNotice,
    PlatformFinalizerError, PlatformFinalizerNotice,
};
use crate::platform::generator::{PlatformGeneratorError, PlatformJsonGeneratorInput};
use crate::{platform, PhpBuildpack};
use ::composer::{ComposerLock, ComposerRootPackage};
use libcnb::build::BuildContext;
use libcnb::Env;
use std::collections::HashMap;
use std::path::Path;
use std::{fs, io};
use url::Url;
use warned::Warned;

pub(crate) struct ProjectLoader {
    composer_json_name: String,
    composer_lock_name: String,
}

impl ProjectLoader {
    pub(crate) fn new(composer_json_name: String, composer_lock_name: String) -> Self {
        Self {
            composer_json_name,
            composer_lock_name,
        }
    }

    pub(crate) fn from_env(env: &Env) -> Self {
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

        Self::new(composer_json_name, composer_lock_name)
    }

    pub(crate) fn detect(&self, project_dir: &Path) -> bool {
        project_dir.join(&self.composer_json_name).exists()
    }

    pub(crate) fn load(&self, project_dir: &Path) -> Result<Project, ProjectLoadError> {
        let composer_json_path = project_dir.join(&self.composer_json_name);
        let composer_lock_path = project_dir.join(&self.composer_lock_name);

        let composer_json =
            fs::read(&composer_json_path).map_err(ProjectLoadError::ComposerJsonRead)?;

        let composer_json = serde_json::from_slice::<ComposerRootPackage>(&composer_json)
            .map_err(ProjectLoadError::ComposerJsonParse)?;

        let composer_lock = match fs::read(composer_lock_path) {
            Ok(json) => Ok(Some(
                serde_json::from_slice(&json).map_err(ProjectLoadError::ComposerLockParse)?,
            )),
            Err(err) => match err.kind() {
                io::ErrorKind::NotFound => Ok(None), // lock does not have to exist
                _ => Err(err),
            },
        }
        .map_err(ProjectLoadError::ComposerLockRead)?;

        Ok(Project::new(
            self.composer_json_name.clone(),
            self.composer_lock_name.clone(),
            composer_json,
            composer_lock,
        ))
    }
}

#[derive(Default)]
pub(crate) struct Project {
    composer_json_name: String,
    composer_lock_name: String,
    composer_json: ComposerRootPackage,
    composer_lock: Option<ComposerLock>,
}

#[derive(Debug)]
pub(crate) enum ProjectLoadError {
    ComposerJsonRead(io::Error),
    ComposerJsonParse(serde_json::Error),
    ComposerLockRead(io::Error),
    ComposerLockParse(serde_json::Error),
}

#[derive(Debug)]
pub(crate) enum PlatformJsonError {
    Extractor(PlatformExtractorError),
    Generator(PlatformGeneratorError),
    Finalizer(PlatformFinalizerError),
}

#[derive(Debug)]
pub(crate) enum PlatformJsonNotice {
    Extractor(PlatformExtractorNotice),
    Finalizer(PlatformFinalizerNotice),
}

impl Project {
    pub(crate) fn new(
        composer_json_name: String,
        composer_lock_name: String,
        composer_json: ComposerRootPackage,
        composer_lock: Option<ComposerLock>,
    ) -> Self {
        Self {
            composer_json_name,
            composer_lock_name,
            composer_json,
            composer_lock,
        }
    }

    pub(crate) fn platform_json(
        &self,
        stack: &str,
        installer_path: &Path,
        platform_repositories: &Vec<Url>,
        dev: bool,
    ) -> Result<Warned<ComposerRootPackage, PlatformJsonNotice>, PlatformJsonError> {
        let mut extractor_notices = Vec::new();
        let generator_input = match &self.composer_lock {
            Some(l) => crate::package_manager::composer::extract_from_lock(l)
                .map_err(PlatformJsonError::Extractor)?,
            None => Warned::from(PlatformJsonGeneratorInput {
                input_name: "auto/generated".to_string(),
                input_revision: "main".to_string(),
                additional_require: Some(HashMap::from([(
                    "heroku-sys/composer".to_string(),
                    "*".to_string(),
                )])),
                ..Default::default()
            }),
        }
        .unwrap(&mut extractor_notices); // Warned::unwrap does not panic :)

        let mut ret = platform::generator::generate_platform_json(
            generator_input,
            stack,
            installer_path,
            platform_repositories,
        )
        .map_err(PlatformJsonError::Generator)?;

        let finalizer_notices =
            crate::package_manager::composer::ensure_runtime_requirement(&mut ret)
                .map_err(PlatformJsonError::Finalizer)?;

        if !dev {
            // we do not want dev requirements to even get resolved, so we do not return them
            // the reason is that even with a --no-dev install, Composer has to resolve all packages, both in require and require-dev
            // but it is common for require-dev to e.g. list ext-xdebug, and if that isn't available in our repositories, even a non-dev install would fail
            ret.package.require_dev.take();
        }

        Ok(Warned::new(
            ret,
            extractor_notices
                .into_iter()
                .map(PlatformJsonNotice::Extractor)
                .chain(
                    finalizer_notices
                        .into_iter()
                        .map(PlatformJsonNotice::Finalizer),
                ),
        ))
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        // TODO: enforce presence of userland composer.lock if composer.json lists requires

        // TODO: call "composer validate"?
        //       ^ yes, also for lockfile freshness check
        //       ^ also as a fallback validation for when we have a Category::Data error

        // FIXME: we have to fail (or warn?) if heroku/heroku-buildpack-php is a dependency

        // TODO: check for presence of `vendor` dir
        // TODO: validate COMPOSER_AUTH?

        Ok(())
    }

    pub(crate) fn install_dependencies(
        &self,
        context: &BuildContext<PhpBuildpack>,
        command_env: &mut Env,
    ) -> Result<(), DependencyInstallationError> {
        crate::package_manager::composer::install_dependencies(&context, command_env)
    }
}
