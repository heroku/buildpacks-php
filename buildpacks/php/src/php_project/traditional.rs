use crate::platform::generator::{
    PlatformFinalizerNotice, PlatformGeneratorError, PlatformJsonGeneratorInput,
};
use crate::{platform, PhpBuildpack};
use composer::ComposerRootPackage;
use libcnb::build::BuildContext;
use libcnb::Env;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use url::Url;

pub(crate) struct Traditional {
    index_file_name: String,
    document_root: Option<String>,
}

impl Traditional {
    pub(crate) fn new(index_file_name: String, document_root: Option<String>) -> Self {
        Self {
            index_file_name,
            document_root,
        }
    }

    pub(crate) fn attempt(project_dir: &Path, env: &Env) -> Result<Self, Vec<String>> {
        // the file name is customizable
        let document_root = env.get_string_lossy("DOCUMENT_ROOT");

        // TODO: warning about legacy projects

        let r = Self::new("index.php".into(), document_root);
        if r.detect(&project_dir) {
            Ok(r)
        } else {
            Err(vec!["FIXME: what do we even say here?".into()])
        }
    }

    pub(crate) fn detect(&self, project_dir: &Path) -> bool {
        project_dir
            .join(&self.document_root.as_deref().unwrap_or(".".into()))
            .join(&self.index_file_name)
            .exists()
    }

    pub(crate) fn make_platform_json(
        &self,
        stack: &str,
        installer_path: &Path,
        platform_repositories: &Vec<Url>,
        _dev: bool,
    ) -> Result<(ComposerRootPackage, HashSet<PlatformFinalizerNotice>), PlatformGeneratorError>
    {
        // TODO: remove composer requirement
        // ^ not yet possible as boot scripts need composer to set COMPOSER_(BIN|VENDOR)_DIR for web server configs
        let generator_input = PlatformJsonGeneratorInput {
            input_name: "auto/generated".to_string(),
            input_revision: "main".to_string(),
            additional_require: Some(HashMap::from([
                ("heroku-sys/composer".to_string(), "*".to_string()),
                ("heroku-sys/php".to_string(), "*".to_string()),
            ])),
            ..Default::default()
        };
        Ok((
            platform::generator::generate_platform_json(
                generator_input,
                stack,
                installer_path,
                platform_repositories,
            )?,
            HashSet::new(),
        ))
    }

    pub(crate) fn install_dependencies(
        &self,
        _context: &BuildContext<PhpBuildpack>,
        _env: &Env,
    ) -> Result<(), String> {
        Ok(())
    }
}
