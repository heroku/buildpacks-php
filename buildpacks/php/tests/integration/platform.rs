use crate::utils::{builder, target_triple};
use indoc::{formatdoc, indoc};
use libcnb_test::{
    BuildConfig, BuildpackReference, TestRunner, assert_contains, assert_contains_match,
};

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
                  - Installing extensions provided by dzuelke/ext-pq-polyfill
                    - ext-raphf {version_triple}
                    - ext-pq {version_triple}
                  - Installing extensions provided by phpseclib/mcrypt_compat
                    NOTICE: No suitable native version of heroku-sys/ext-mcrypt available
                  - Installing extensions provided by symfony/polyfill-ctype
                    - ext-ctype {enabled}
                  - Installing extensions provided by symfony/polyfill-mbstring
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

#[test]
#[ignore = "integration test"]
fn platform_test_failure() {
    let build_config = BuildConfig::new(builder(), "tests/fixtures/platform/installation/failure")
        .buildpacks(vec![BuildpackReference::CurrentCrate])
        .target_triple(target_triple(builder()))
        .expected_pack_result(libcnb_test::PackResult::Failure)
        .to_owned();

    TestRunner::default().build(&build_config, |context| {
        assert_contains!(
            context.pack_stdout,
            indoc! {r"
                ! Failed to install platform dependencies
                !
                ! Your platform requirements (for runtimes and extensions) could
                ! not be resolved to an installable set of dependencies.
                !
                ! This usually means that you (or packages you are using) depend
                ! on a combination of PHP versions and/or extensions that are
                ! currently not available on Heroku.
                !
                ! The following is the full output from the installation attempt:
                !
                ! > Loading repositories with available runtimes and extensions
                ! > Your requirements could not be resolved to an installable set of packages.
                ! > 
                ! >   Problem 1
                ! >     - Root composer.json requires ext-doesnotexist, it could not be found in any version, there may be a typo in the package name.
                ! > 
                !
                ! Please verify that all requirements for runtime versions in
                ! 'composer.lock' are compatible with the list below, and ensure
                ! all required extensions are available for the desired runtimes.
                !
                ! When choosing a PHP runtimes and extensions, please also ensure
                ! they are available on your app's stack, and, if necessary, choose
                ! a different stack after consulting the article below.
                !
                ! For a list of supported runtimes & extensions on Heroku, please
                ! refer to: https://devcenter.heroku.com/articles/php-support
                "
            }
        );
    });
}
