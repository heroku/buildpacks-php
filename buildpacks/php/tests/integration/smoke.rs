//! Smoke tests that ensure a set of basic apps build successfully and the resulting container
//! exposes the HTTP interface of that app as expected. They also re-build the app and assert the
//! resulting container again to ensure that potential caching logic in the buildpack does not
//! break subsequent builds.
//!
//! These tests are strictly happy-path tests and do not assert any output of the buildpack.

use crate::utils::{
    builder, default_buildpacks, set_php_version, smoke_test,
    start_container_assert_basic_http_response, target_triple,
};
use fs_err as fs;
use libcnb_test::{assert_contains, BuildConfig, BuildpackReference, TestRunner};
use serde_json::json;

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
fn php_8_3() {
    let version_req = "8.3.*";
    let build_config = BuildConfig::new(builder(), "tests/fixtures/smoke/hello-world")
        .buildpacks(vec![BuildpackReference::CurrentCrate])
        .target_triple(target_triple(builder()))
        .app_dir_preprocessor(move |app_dir| set_php_version(&app_dir, version_req))
        .to_owned();

    TestRunner::default().build(&build_config, |context| {
        assert_contains!(context.run_shell_command("php --version").stdout, "PHP 8.3");
        start_container_assert_basic_http_response(&context, "Hello World");
    });
}

#[test]
#[ignore = "integration test"]
fn php_8_4() {
    let version_req = "8.4.*";
    let build_config = BuildConfig::new(builder(), "tests/fixtures/smoke/hello-world")
        .buildpacks(vec![BuildpackReference::CurrentCrate])
        .target_triple(target_triple(builder()))
        .app_dir_preprocessor(move |app_dir| set_php_version(&app_dir, version_req))
        .to_owned();

    TestRunner::default().build(&build_config, |context| {
        assert_contains!(context.run_shell_command("php --version").stdout, "PHP 8.4");
        start_container_assert_basic_http_response(&context, "Hello World");
    });
}

#[test]
#[ignore = "integration test"]
fn smoke_test_composer_json_scripts_as_objects() {
    let build_config = BuildConfig::new(builder(), "tests/fixtures/smoke/hello-world")
        .buildpacks(vec![BuildpackReference::CurrentCrate])
        .target_triple(target_triple(builder()))
        .app_dir_preprocessor(|app_dir| {
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
        })
        .to_owned();

    TestRunner::default().build(&build_config, |_context| {});
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
