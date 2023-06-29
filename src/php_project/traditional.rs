use crate::composer::platform::{PlatformGeneratorError, PlatformGeneratorNotice};
use crate::composer::ComposerRootPackage;
use crate::layers::php::PhpLayerMetadata;
use crate::{composer, PhpBuildpack};
use libcnb::build::BuildContext;
use libcnb::layer::LayerData;
use libcnb::Env;
use std::collections::HashSet;
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
        dev: bool,
    ) -> Result<(ComposerRootPackage, HashSet<PlatformGeneratorNotice>), PlatformGeneratorError>
    {
        let lock = composer::ComposerLock::new(Some("2.99.0".into()));

        // TODO: make more minimal JSON that doesn't pull in Composer
        // ^ not yet possible as boot scripts need composer to set COMPOSER_(BIN|VENDOR)_DIR for web server configs
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
        Ok(())
    }
}
