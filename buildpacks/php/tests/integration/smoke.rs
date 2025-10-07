//! Smoke tests that ensure a set of basic apps build successfully and the resulting container
//! exposes the HTTP interface of that app as expected. They also re-build the app and assert the
//! resulting container again to ensure that potential caching logic in the buildpack does not
//! break subsequent builds.
//!
//! These tests are strictly happy-path tests and do not assert any output of the buildpack.

use crate::utils::{
    builder, default_buildpacks, smoke_test, start_container_assert_basic_http_response,
    target_triple,
};
use fs_err as fs;
use indoc::formatdoc;
use libcnb_test::{BuildConfig, BuildpackReference, TestRunner, assert_contains_match};
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
fn smoke_test_php_nginx() {
    let build_config = BuildConfig::new(builder(), "tests/fixtures/smoke/hello-world")
        .buildpacks(vec![BuildpackReference::CurrentCrate])
        .target_triple(target_triple(builder()))
        .app_dir_preprocessor(|app_dir| {
            fs::write(app_dir.join("Procfile"), "web: heroku-php-nginx").unwrap();
        })
        .to_owned();

    TestRunner::default().build(&build_config, |context| {
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

#[test]
#[ignore = "integration test"]
fn smoke_test_php_polyfills() {
    let build_config = BuildConfig::new(builder(), "tests/fixtures/smoke/polyfills")
        .buildpacks(vec![BuildpackReference::CurrentCrate])
        .target_triple(target_triple(builder()))
        .to_owned();

    TestRunner::default().build(&build_config, |context| {
        assert_contains_match!(
            context.pack_stdout,
            formatdoc! {r"
                - Installing platform packages
                  - php {version_triple}
                  - composer {version_triple}
                  - ext-bcmath {bundled}
                  - ext-gd {bundled}
                  - ext-imagick {version_triple}
                  - ext-intl {bundled}
                  - ext-oauth {version_triple}
                  - ext-redis {version_triple}
                  - ext-soap {bundled}
                  - Attempting native package installs for dzuelke/ext-pq-polyfill
                    - ext-raphf {version_triple}
                    - ext-pq {version_triple}
                  - Attempting native package installs for phpseclib/mcrypt_compat
                    - No suitable native version of heroku-sys/ext-mcrypt available
                  - Attempting native package installs for symfony/polyfill-ctype
                    - ext-ctype {enabled}
                  - Attempting native package installs for symfony/polyfill-mbstring
                    - ext-mbstring {bundled}
                - Installing web servers
                  - nginx {version_triple}
                  - apache {version_triple}
                  - boot-scripts {version_triple}
                ",
                version_triple = r"\(\d+\.\d+\.\d+\)",
                bundled = r"\(bundled with php\)",
                enabled = r"\(already enabled\)"
            }
        );
    });
}
