//! Smoke tests that ensure a set of basic apps build successfully and the resulting container
//! exposes the HTTP interface of that app as expected. They also re-build the app and assert the
//! resulting container again to ensure that potential caching logic in the buildpack does not
//! break subsequent builds.
//!
//! These tests are strictly happy-path tests and do not assert any output of the buildpack.

use crate::utils::{builder, copy_dir_all, default_buildpacks, smoke_test, target_triple};
use indoc::formatdoc;
use libcnb_test::{BuildConfig, BuildpackReference, TestRunner};
use serde_json::json;
use std::fs;
use std::path::PathBuf;

#[test]
#[ignore = "integration test"]
fn smoke_test_bundled_hello_world_app() {
    smoke_test(
        builder(),
        "tests/fixtures/smoke/hello-world",
        vec![BuildpackReference::CurrentCrate],
        "Hello World",
    );
}

#[test]
#[ignore = "integration test"]
fn smoke_test_composer_json_scripts_as_objects() {
    let temp = tempfile::tempdir().unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/smoke/hello-world")
        .canonicalize()
        .unwrap();
    let app_dir = temp.path();
    copy_dir_all(source, app_dir).unwrap();

    let mut composer_json = serde_json::from_str::<serde_json::Map<_, _>>(
        &fs::read_to_string(app_dir.join("composer.json")).unwrap(),
    )
    .unwrap();

    composer_json.insert(
        "scripts".to_string(),
        json!({
            "auto-scripts": {
                "cache:clear": "echo 'cache:clear'",
                "assets:install %PUBLIC_DIR%": "echo 'assets:install'",
                "importmap:install": "echo 'importmap:install'"
            },
            "post-install-cmd": [
                "@auto-scripts"
            ],
            "post-update-cmd": [
                "@auto-scripts"
            ]
        }),
    );
    fs::write(
        app_dir.join("composer.json"),
        serde_json::to_string(&composer_json).unwrap(),
    )
    .unwrap();

    let build_config = BuildConfig::new(builder(), app_dir)
        .buildpacks(vec![BuildpackReference::CurrentCrate])
        .target_triple(target_triple(builder()))
        .to_owned();

    TestRunner::default().build(&build_config, |context| {
        assert_eq!(
            formatdoc! {"
                /layers/heroku_php/platform/bin/php
                /layers/heroku_php/platform/bin/php
                /layers/heroku_php/platform/bin/php
            "}
            .trim(),
            context.run_shell_command("which -a php").stdout.trim()
        );
    });
}

#[test]
#[ignore = "integration test"]
fn smoke_test_php_getting_started() {
    smoke_test(
        builder(),
        "tests/fixtures/smoke/heroku-php-getting-started",
        default_buildpacks(),
        "Getting Started with PHP on Heroku",
    );
}
