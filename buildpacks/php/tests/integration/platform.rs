use crate::utils::{builder, target_triple};
use indoc::formatdoc;
use libcnb_test::{BuildConfig, BuildpackReference, TestRunner, assert_contains_match};

#[test]
#[ignore = "integration test"]
fn platform_test_polyfills() {
    let build_config =
        BuildConfig::new(builder(), "tests/fixtures/platform/installation/polyfills")
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
