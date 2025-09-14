mod generator;

use figment::value::magic::RelativePathBuf;
use serde::{Deserialize, Serialize};
use std::path::Path;
use url::Url;

#[derive(Deserialize, Serialize)]
struct ComposerLockTestCaseConfig {
    name: Option<String>,
    description: Option<String>,
    lock: Option<RelativePathBuf>,
    stack: String,
    expected_result: Option<RelativePathBuf>,
    expected_extractor_notices: Option<Vec<String>>,
    expected_finalizer_notices: Option<Vec<String>>,
    expect_extractor_failure: Option<String>,
    expect_generator_failure: Option<String>,
    expect_finalizer_failure: Option<String>,
    install_dev: bool,
    repositories: Vec<Url>,
}

impl Default for ComposerLockTestCaseConfig {
    fn default() -> Self {
        let stack = "heroku-20";
        Self {
            name: None,
            description: None,
            lock: None,
            stack: stack.to_string(),
            expected_result: None,
            expected_extractor_notices: None,
            expected_finalizer_notices: None,
            expect_generator_failure: None,
            expect_extractor_failure: None,
            expect_finalizer_failure: None,
            install_dev: false,
            repositories: vec![
                Url::parse(&format!(
                    "https://lang-php.s3.us-east-1.amazonaws.com/dist-{stack}-cnb/packages.json",
                ))
                .unwrap(),
            ],
        }
    }
}

impl<P: AsRef<Path>> From<P> for ComposerLockTestCaseConfig {
    fn from(p: P) -> Self {
        let dir = p.as_ref();
        let lock = dir.join("composer.lock");
        let expected_result = dir.join("expected_platform_composer.json");
        Self {
            name: Some(dir.file_name().unwrap().to_string_lossy().to_string()),
            lock: lock.try_exists().unwrap().then_some(lock.into()),
            expected_result: expected_result
                .try_exists()
                .unwrap()
                .then_some(expected_result.into()),
            ..Default::default()
        }
    }
}
