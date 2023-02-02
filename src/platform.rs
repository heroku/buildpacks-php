use crate::layers::bootstrap;
use libcnb::data::buildpack::StackId;
use libcnb::Env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::string::String;

pub(crate) fn generate_platform_json(
    stack_id: &StackId,
    app_dir: &Path,
    bootstrap_path: &PathBuf,
    bootstrap_env: &Env,
    platform_repository_urls: Vec<String>,
) -> Result<String, PlatformGeneratorError> {
    let output = Command::new("php")
        .args([
            &bootstrap_path
                .join(bootstrap::INSTALLER_SUBDIR)
                .join("bin/util/platform.php"),
            &bootstrap_path
                .join(bootstrap::INSTALLER_SUBDIR)
                .join("support/installer/"),
        ])
        .args(platform_repository_urls)
        .current_dir(app_dir)
        .env_clear()
        .envs(bootstrap_env)
        .env("STACK", stack_id.to_string())
        .output()
        .expect("Failed to execute platform.php");
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into())
    } else {
        match output.status.code() {
            Some(1) => Err(PlatformGeneratorError::Parse {
                message: String::from_utf8_lossy(&output.stderr).into(),
            }),
            Some(3) => Err(PlatformGeneratorError::OnlyDevRequireInRuntime),
            Some(4) => Err(PlatformGeneratorError::Repository {
                message: String::from_utf8_lossy(&output.stderr).into(),
            }),
            Some(code) => Err(PlatformGeneratorError::Unknown {
                code,
                message: String::from_utf8_lossy(&output.stderr).into(),
            }),
            None => Err(PlatformGeneratorError::Terminated),
        }
    }
}

#[derive(Debug)]
#[allow(unused)]
pub(crate) enum PlatformGeneratorError {
    Parse {
        // 1
        message: String,
    },
    OnlyDevRequireInRuntime, // 3
    Repository {
        // 4
        message: String,
    },
    Terminated,
    Unknown {
        code: i32,
        message: String,
    },
}
