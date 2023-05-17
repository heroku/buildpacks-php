mod composer;
mod traditional;

use crate::composer::ComposerRootPackage;
use crate::platform::generator::{PlatformGeneratorError, PlatformGeneratorNotice};
use crate::PhpBuildpack;
use libcnb::build::BuildContext;
use libcnb::Env;
use std::collections::HashSet;
use std::path::Path;
use url::Url;

pub(crate) enum PhpProject {
    Traditional(traditional::Traditional),
    Composer(composer::Composer),
}

impl PhpProject {
    pub(crate) fn detect(project_dir: &Path, env: &Env) -> Option<Self> {
        if let Ok(v) = composer::Composer::attempt(project_dir, &env) {
            return Some(Self::Composer(v));
        }

        if let Ok(v) = traditional::Traditional::attempt(project_dir, &env) {
            return Some(Self::Traditional(v));
        }

        None
    }

    pub(crate) fn load(&mut self, project_dir: &Path) -> Result<(), String> {
        match self {
            PhpProject::Composer(ref mut composer) => composer.load(project_dir),
            _ => Ok(()),
        }
    }

    pub(crate) fn make_platform_json(
        &self,
        stack: &str,
        installer_path: &Path,
        platform_repositories: &Vec<Url>,
        dev: bool,
    ) -> Result<(ComposerRootPackage, HashSet<PlatformGeneratorNotice>), PlatformGeneratorError>
    {
        match self {
            PhpProject::Composer(p) => {
                p.make_platform_json(stack, installer_path, platform_repositories, dev)
            }
            PhpProject::Traditional(p) => {
                p.make_platform_json(stack, installer_path, platform_repositories, dev)
            }
        }
    }

    pub(crate) fn install_dependencies(
        &self,
        context: &BuildContext<PhpBuildpack>,
        command_env: &mut Env,
    ) -> Result<(), String> {
        match self {
            PhpProject::Composer(p) => p.install_dependencies(&context, command_env),
            PhpProject::Traditional(p) => p.install_dependencies(&context, command_env),
        }
    }
}
