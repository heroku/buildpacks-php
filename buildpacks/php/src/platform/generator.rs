use std::collections::{HashMap, HashSet};
use std::ops::Not;
use std::path::Path;
use std::string::ToString;

use chrono::offset::Utc;
use composer::{
    ComposerBasePackage, ComposerLock, ComposerPackage, ComposerRepository,
    ComposerRepositoryFilters, ComposerRootPackage,
};
use monostate::MustBe;
use regex::Regex;
use serde_json::{json, Value};
use url::Url;

fn ensure_heroku_sys_prefix(name: impl AsRef<str>) -> String {
    let name = name.as_ref();
    format!(
        "heroku-sys/{}",
        name.strip_prefix("heroku-sys/").unwrap_or(name)
    )
}

fn split_and_trim_list<'a>(list: &'a str, sep: &'a str) -> impl Iterator<Item = &'a str> {
    list.split(sep)
        .map(str::trim)
        .filter_map(|p| (!p.is_empty()).then_some(p))
}

/// Parse a given repository [`Url`] with optional priority and filter query args into a [`ComposerRepository`].
///
/// To allow users to specify whether or not a repository is canonical, or filters for packages,
/// as documented at <https://getcomposer.org/doc/articles/repository-priorities.md>, the following
/// URL query arguments are available:
/// - `composer-repository-canonical` (`true` or `false`)
/// - `composer-repository-exclude` (comma-separated list of package names)
/// - `composer-repository-only` (comma-separated list of package names)
///
/// These query args, if present, are not removed from the URL written to the [`ComposerRepository`]
/// to ensure that a possible signature included in the URL string remains valid.
fn composer_repository_from_repository_url(
    url: Url,
) -> Result<ComposerRepository, ComposerRepositoryFromRepositoryUrlError> {
    const CANONICAL_QUERY_ARG_NAME: &str = "composer-repository-canonical";
    const ONLY_QUERY_ARG_NAME: &str = "composer-repository-only";
    const EXCLUDE_QUERY_ARG_NAME: &str = "composer-repository-exclude";

    let mut canonical = None;
    let mut filters = None;
    for (k, v) in url.query_pairs() {
        let k = k.as_ref();
        let v = v.as_ref();
        match k {
            CANONICAL_QUERY_ARG_NAME => {
                canonical = Some(matches!(
                    v.trim().to_ascii_lowercase().as_ref(),
                    "1" | "true" | "on" | "yes"
                ))
            }
            ONLY_QUERY_ARG_NAME | EXCLUDE_QUERY_ARG_NAME => {
                if filters.is_some() {
                    return Err(ComposerRepositoryFromRepositoryUrlError::MultipleFilters);
                };
                let filter_list = split_and_trim_list(v, ",")
                    .map(ensure_heroku_sys_prefix)
                    .collect();
                filters = Some(match k {
                    ONLY_QUERY_ARG_NAME => ComposerRepositoryFilters::Only(filter_list),
                    EXCLUDE_QUERY_ARG_NAME => ComposerRepositoryFilters::Exclude(filter_list),
                    _ => unreachable!(),
                });
            }
            _ => (),
        }
    }

    Ok(ComposerRepository::Composer {
        kind: Default::default(),
        url,
        allow_ssl_downgrade: None,
        force_lazy_providers: None,
        options: None,
        canonical,
        filters,
    })
}
#[derive(Debug)]
enum ComposerRepositoryFromRepositoryUrlError {
    MultipleFilters,
}

fn is_platform_package(name: impl AsRef<str>) -> bool {
    let name = name.as_ref();
    // same regex used by Composer as well
    Regex::new(r"^(?i)(?:php(?:-64bit|-ipv6|-zts|-debug)?|hhvm|(?:ext|lib)-[a-z0-9](?:[_.-]?[a-z0-9]+)*|composer(?:-(?:plugin|runtime)-api)?)$")
        .expect(
            "You've got a typo in that regular expression. No, it was not broken before. Yes, I am sure.",
        )
        .is_match(name)
        // ext-….native packages are ours, and ours alone - virtual packages to later force installation of native extensions in case of userland "provide"s 
        && !(name.starts_with("ext-") && name.ends_with(".native"))
        // we ignore those for the moment - they're not in package metadata (yet), and if they were, the versions are "frozen" at build time, but stack images get updates...
        && !name.starts_with("lib-")
        // not currently in package metadata
        // TODO: put into package metadata so it's usable
        && name != "composer-runtime-api"
}

fn has_runtime_requirement(requires: &HashMap<String, String>) -> bool {
    requires.contains_key("heroku-sys/php")
}

fn extract_platform_links_with_heroku_sys<T: Clone>(
    links: &HashMap<String, T>,
) -> Option<HashMap<String, T>> {
    let ret = links
        .iter()
        .filter(|(k, _)| is_platform_package(k))
        .map(|(k, v)| (ensure_heroku_sys_prefix(k), v.clone()))
        .collect::<HashMap<_, _>>();

    ret.is_empty().not().then_some(ret)
}

#[derive(strum_macros::Display, Debug, Eq, PartialEq)]
pub(crate) enum PlatformGeneratorError {
    EmptyPlatformRepositoriesList,
    InvalidRepositoryFilter,
    InvalidStackIdentifier,
    InvalidPlatformApiVersion,
    RuntimeRequirementInRequireDevButNotRequire,
}
#[derive(strum_macros::Display, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PlatformGeneratorNotice {
    NoComposerPluginApiVersionInLock(String),
    ComposerPluginApiVersionConfined(String, String),
    RuntimeRequirementInserted(String),
    RuntimeRequirementFromDependencies,
}
pub(crate) fn generate_platform_json(
    lock: &ComposerLock,
    stack: &str,
    installer_path: &Path,
    platform_repositories: &Vec<Url>,
    dev: bool,
) -> Result<(ComposerRootPackage, HashSet<PlatformGeneratorNotice>), PlatformGeneratorError> {
    if platform_repositories.is_empty() {
        return Err(PlatformGeneratorError::EmptyPlatformRepositoriesList);
    };

    let mut notices = HashSet::new();

    let Value::Object(config) = json!({
        "cache-files-ttl": 0,
        "discard-changes": true,
        "allow-plugins": {
            "heroku/installer-plugin": true,
        },
    }) else {
        unreachable!();
    };

    let caps = Regex::new(r"^([^-]+)(?:-([0-9]+))?$")
        .expect("A certain somebody broke the stack parsing regex. Yes, I am looking at you.")
        .captures(stack)
        .ok_or(PlatformGeneratorError::InvalidStackIdentifier)?;
    let stack_provide = (
        format!("heroku-sys/{}", String::from(caps.get(1).unwrap().as_str())),
        format!(
            "{}.{}",
            caps.get(2).map_or("1", |m| m.as_str()),
            Utc::now().format("%Y.%0m.%0d")
        ),
    );

    let mut requires: HashMap<String, String> = [
        // our installer plugin - individual platform packages also require it, but hey
        ("heroku/installer-plugin", "*"),
        ("heroku-sys/apache", "^2.4.10"),
        ("heroku-sys/nginx", "^1.8.0"),
        // we want the latest Composer...
        ("heroku-sys/composer", "*"),
    ]
    .iter()
    .map(|v| (v.0.into(), v.1.into()))
    .collect();

    let mut dev_requires: HashMap<String, String> = HashMap::new();

    // ... that supports the major plugin API version from the lock file (which corresponds to the Composer version series, so e.g. all 2.3.x releases have 2.3.0)
    // if the lock file says "2.99.0", we generally still want to select "^2", and not "^2.99.0"
    // this is so the currently available Composer version can install lock files generated by brand new or pre-release Composer versions, as this stuff is generally forward compatible
    // otherwise, builds would fail the moment e.g. 2.6.0 comes out and people try it, even though 2.5 could install the project just fine
    requires.insert(
        "heroku-sys/composer-plugin-api".into(),
        match lock.plugin_api_version.as_deref() {
            // no rule without an exception, of course:
            // there are quite a lot of BC breaks for plugins in Composer 2.3
            // if the lock file was generated with 2.0, 2.1 or 2.2, we play it safe and install 2.2.x (which is LTS)
            // this is mostly to ensure any plugins that have an open enough version selector do not break with all the 2.3 changes
            // also ensures plugins are compatible with other libraries Composer bundles (e.g. various Symfony components), as those got big version bumps in 2.3
            Some(v @ ("2.0.0" | "2.1.0" | "2.2.0")) => {
                let r = "~2.2.0".to_string();
                notices.insert(PlatformGeneratorNotice::ComposerPluginApiVersionConfined(
                    v.to_string(),
                    r.clone(),
                ));
                r
            }
            // just "^2" or similar so we get the latest we have, see comment earlier
            Some(v) => format!(
                "^{}",
                v.split_once('.')
                    .ok_or(PlatformGeneratorError::InvalidPlatformApiVersion)?
                    .0
            ),
            // nothing means it's pre-v1.10, in which case we want to just use v1
            None => {
                let r = "^1.0.0".to_string();
                notices.insert(PlatformGeneratorNotice::NoComposerPluginApiVersionInLock(
                    r.clone(),
                ));
                r
            }
        },
    );

    let mut repositories = vec![
        ComposerRepository::Disabled(HashMap::from([("packagist.org".into(), MustBe!(false))])),
        // our heroku/installer-plugin
        ComposerRepository::from_path_with_options(
            installer_path,
            [("symlink".into(), Value::Bool(false))],
        ),
    ];

    // for now, we need the web server boot scripts and configs from the classic buildpack
    // so we install it as a package from a relative location - it's "above" the installer path
    requires.insert("heroku/heroku-buildpack-php".into(), "dev-master".into());
    let mut classic_buildpack_repo = ComposerRepository::from_path_with_options(
        installer_path.join("../.."),
        [("symlink".into(), Value::Bool(false))],
    );
    // FIXME: does this need error handling? or an else? not very readable IMO
    if let ComposerRepository::Path {
        ref mut options, ..
    } = classic_buildpack_repo
    {
        options.get_or_insert(Default::default()).insert(
            "versions".into(),
            json!({"heroku/heroku-buildpack-php": "dev-master"}),
        );
    }
    repositories.push(classic_buildpack_repo);

    let mut composer_repositories = platform_repositories
        .into_iter()
        .map(|url| {
            composer_repository_from_repository_url(url.clone())
                .map_err(|_| PlatformGeneratorError::InvalidRepositoryFilter)
        })
        // repositories are passed in in ascending order of precedence
        // typically our default repo first, then user-supplied repos after that
        // by default, repositories are canonical, so lookups will not happen in later repos if a package is found in an earlier repo
        // (even if the later repo has newer, or better matching for other requirements, versions)
        // so we reverse the list to allow later repositories to overwrite packages from earlier ones
        // users can still have Composer fall back to e.g. the default repo for newer versions using ?composer-repository-canonical=false
        .rev()
        .collect::<Result<Vec<ComposerRepository>, _>>()?;

    repositories.append(&mut composer_repositories);

    // we take all requirements from platform(-dev) and move them into a "composer.json/composer.lock(-require-dev)" metapackage
    // ^ this package gets those platform requirements as its "require"s, with a "heroku-sys/" prefix, of course
    // ^ this is done because if the root package requires an extension, and there is also a dependency on a polyfill for it,
    //   e.g. "heroku-sys/ext-mbstring" and "symfony/polyfill-mbstring" (which declares "ext-mbstring" as "provide"d),
    //   there would be no way to know that anything required "ext-mbstring", since the solver optimizes this away,
    //   and no install event is fired for the root package itself
    // ^ we do however need to know this, since we have to attempt an install of the "real", "native" extension later
    // > the solution is to wrap the platform requirements into a metapackage, for which an install event fires, where we can extract these requirements
    //
    // due to the above, we also have to check if there is a stability-flags entry for each of the platform(-dev) requires
    // ^ if so, create a dummy require(-dev) for that package with just @beta etc
    // ^ we do that because the actual require of the package will be in that "composer.json/composer.lock" package mentioned earlier,
    //   but stability flags are ignored in requirements that are not in the root package's require section
    // ^ this is done intentionally by Composer to prevent dependencies from pushing unstable stuff onto users without explicit opt-in
    //
    // for all packages(-dev), create a type=metapackage for the package repo
    // ^ with name and version copied
    // ^ with require, replace, provide and conflict entries that reference a platform package rewritten with heroku-sys/ prefix
    //
    // even without dev installs, we process all platform-dev and packages-dev packages so we can tell at the end if there is no version requirement in all of require, but in require-dev
    // ^ to ensure folks get the same PHP version etc as locally/CI
    // ^ but we will not write the collected requirements into require-dev, because "composer update" has to check "require-dev" packages to ensure lock file consistency even if "--no-dev" is used
    //   (--no-dev only affects dependency installation, not overall dependency resolution)
    //   (and people frequently have e.g. ext-xdebug as a dev requirement)
    //
    // through all this, record whether we've seen a "php" entry yet for "platform/packages" and "platform-dev/packages-dev"
    // ^ if no for "platform/packages" but yes for "platform-dev/packages-dev", fail with error
    // ^ otherwise, insert a default require
    // ^ but notice if no require in the root, only dependencies
    // ^ TODO: if no, but have seen `php-…` variants like `php-ipv6` or `php-zts`, should we warn or fail?

    let mut seen_runtime_requirement = false;
    let mut seen_runtime_dev_requirement = false;
    let mut runtime_require_in_root = false;

    for (is_dev, platform, packages, requires, marker) in [
        (
            false,
            &lock.platform,
            &lock.packages,
            &mut requires,
            &mut seen_runtime_requirement,
        ),
        (
            true,
            &lock.platform_dev,
            &lock.packages_dev,
            &mut dev_requires,
            &mut seen_runtime_dev_requirement,
        ),
    ] {
        let mut metapaks = Vec::new();
        // first, for the root platform requires from "platform"/"platform-dev", make a special package
        if let Some(root_platform_requires) = extract_platform_links_with_heroku_sys(platform) {
            runtime_require_in_root |= !is_dev && has_runtime_requirement(&root_platform_requires); // we use this later to warn if no "php" requirement

            extract_platform_links_with_heroku_sys(&lock.stability_flags)
                .unwrap_or_default()
                .iter()
                .for_each(|(package_name, numeric_stability)| {
                    requires.insert(
                        package_name.clone(),
                        format!("@{}", &numeric_stability.to_string()),
                    );
                });

            metapaks.push(ComposerPackage {
                name: format!(
                    "composer.json/composer.lock{}",
                    is_dev.then_some("-require-dev").unwrap_or_default() // different names for require and require-dev
                ),
                version: format!("dev-{}", lock.content_hash),
                package: ComposerBasePackage {
                    kind: Some("metapackage".into()),
                    require: Some(root_platform_requires),
                    ..Default::default()
                },
                ..Default::default()
            });
        }

        // then, build packages for all regular requires
        metapaks.extend(packages.iter().filter_map(|package| {
            let require = package
                .package
                .require
                .as_ref()
                .and_then(extract_platform_links_with_heroku_sys);

            let provide = package
                .package
                .provide
                .as_ref()
                .and_then(extract_platform_links_with_heroku_sys);

            let conflict = package
                .package
                .conflict
                .as_ref()
                .and_then(extract_platform_links_with_heroku_sys);

            let replace = package
                .package
                .replace
                .as_ref()
                .and_then(extract_platform_links_with_heroku_sys);

            let has_links = [&require, &provide, &conflict, &replace]
                .into_iter()
                .any(Option::is_some);
            has_links.then(|| ComposerPackage {
                name: package.name.clone(),
                version: package.version.clone(),
                package: ComposerBasePackage {
                    kind: Some("metapackage".into()),
                    require,
                    provide,
                    conflict,
                    replace,
                    ..Default::default()
                },
                ..Default::default()
            })
        }));

        if metapaks.is_empty().not() {
            *marker |= metapaks.iter().any(|package| {
                package
                    .package
                    .require
                    .as_ref()
                    .filter(|requires| has_runtime_requirement(&requires))
                    .is_some()
            });

            // now insert a dependency into the root for each require
            for package in &metapaks {
                requires.insert(package.name.clone(), package.version.clone());
            }
            repositories.push(metapaks.into());
        }
    }

    if !seen_runtime_requirement {
        if seen_runtime_dev_requirement {
            return Err(PlatformGeneratorError::RuntimeRequirementInRequireDevButNotRequire);
        }
        let r = "*".to_string(); // TODO: is * the right value? we used to depend on stack version
        notices.insert(PlatformGeneratorNotice::RuntimeRequirementInserted(
            r.clone(),
        ));
        requires.insert("heroku-sys/php".into(), r);
    } else if !runtime_require_in_root {
        // runtime requirements from dependencies will be used
        notices.insert(PlatformGeneratorNotice::RuntimeRequirementFromDependencies);
    }

    // TODO: generate conflict entries for php-zts, php-debug?
    // (hashset with (name, dev) is then easiest for lookup)

    Ok((
        ComposerRootPackage {
            config: Some(config),
            minimum_stability: Some(lock.minimum_stability.clone()),
            prefer_stable: Some(lock.prefer_stable.clone()),
            package: ComposerBasePackage {
                provide: Some(HashMap::from([stack_provide])),
                replace: None, // TODO: blackfire
                repositories: Some(repositories),
                require: Some(requires),
                // TODO: maybe we should always return these, but on `composer install` failure, have the caller warn, then re-try without them?
                require_dev: (dev && !dev_requires.is_empty()).then_some(dev_requires),
                ..Default::default()
            },
            ..Default::default()
        },
        notices,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::{env, fs};

    use crate::platform::repos_from_defaults_and_list;
    use figment::providers::{Format, Serialized, Toml};
    use figment::{value::magic::RelativePathBuf, Figment};
    use serde::{Deserialize, Serialize};
    use serde_json::Map;

    #[derive(Deserialize, Serialize)]
    struct ComposerLockTestCaseConfig {
        name: Option<String>,
        description: Option<String>,
        lock: Option<RelativePathBuf>, // FIXME: better?
        stack: String,
        expected_result: Option<RelativePathBuf>,
        expected_notices: Option<Vec<String>>, // TODO: can we use MakePlatformJsonError?
        expect_failure: Option<String>,        // TODO: can we use MakePlatformJsonError?
        install_dev: bool,
        repositories: Vec<Url>, // TODO: change to url::Url?
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
                expected_notices: None,
                expect_failure: None,
                install_dev: false,
                repositories: vec![Url::parse(&format!(
                    "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-cnb/packages.json",
                    stack,
                ))
                .unwrap()],
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

    #[test]
    fn make_platform_json_with_fixtures() {
        let installer_path = &PathBuf::from("../../support/installer");

        fs::read_dir(&Path::new("tests/fixtures/platform/generator"))
            .unwrap()
            .filter(|der| der.as_ref().unwrap().metadata().unwrap().is_dir())
            .filter_map(|der| {
                let p = der.unwrap().path();
                // merge our auto-built config (from Path) and a config.toml, if it exists
                let case: ComposerLockTestCaseConfig =
                    Figment::from(Serialized::defaults(ComposerLockTestCaseConfig::from(&p)))
                        .merge(Toml::file(p.join("config.toml")))
                        .extract()
                        .unwrap();
                // skip if there isn't even a lock file in the dir
                case.lock.is_some().then_some(case)
            })
            .for_each(|case| {
                let lock = serde_json::from_str(
                    // .relative() will allow specifying the file name in the config
                    &fs::read_to_string(case.lock.as_ref().unwrap().relative()).unwrap(),
                )
                .unwrap();

                let generated_json_package = generate_platform_json(
                    &lock,
                    &case.stack,
                    &installer_path,
                    &case.repositories,
                    case.install_dev,
                );

                // first check: was this even supposed to succeed or fail?
                assert_eq!(
                    generated_json_package.is_ok(),
                    (case.expected_result.is_some() && case.expect_failure.is_none()),
                    "case {} expected to {}, but it didn't",
                    case.name.as_ref().unwrap(),
                    generated_json_package
                        .is_ok()
                        .then_some("fail")
                        .unwrap_or("succeed"),
                );

                // on failure, check if the type of failure what was the test expected
                let (generated_json_package, generated_json_notices) = match generated_json_package
                {
                    Ok(v) => v,
                    Err(e) => {
                        assert!(
                            case.expect_failure.is_some(),
                            "case {}: failed, but config has no expect_failure type specified",
                            case.name.as_ref().unwrap()
                        );

                        assert_eq!(
                            e.to_string(),
                            case.expect_failure.unwrap(),
                            "case {}: failed as expected, but with mismatched failure type",
                            case.name.as_ref().unwrap()
                        );

                        return;
                    }
                };

                // fetch all notices and compare them against the list of expected notices
                assert_eq!(
                    generated_json_notices
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<HashSet<String>>(),
                    case.expected_notices
                        .unwrap_or_default()
                        .into_iter()
                        .collect::<HashSet<String>>(),
                    "case {}: mismatched notices (left = generated, right = expected)",
                    case.name.as_ref().unwrap()
                );

                let mut expected_json_object: Map<String, Value> = serde_json::from_str(
                    &fs::read_to_string(case.expected_result.unwrap().relative()).unwrap(),
                )
                .unwrap();

                let generated_json_value =
                    serde_json::value::to_value(&generated_json_package).unwrap();
                let generated_json_object = generated_json_value.as_object().unwrap();

                let generated_keys: HashSet<String> =
                    generated_json_object.keys().cloned().collect();
                let expected_keys: HashSet<String> = expected_json_object.keys().cloned().collect();

                // check if all of the expected keys are there (and only those)
                assert_eq!(
                    &generated_keys,
                    &expected_keys,
                    "case {}: mismatched keys (left = generated, right = expected)",
                    case.name.as_ref().unwrap()
                );

                // validate each key in the generated JSON
                // we have to do this because we want to treat e.g. the "provide" key a bit differently
                for key in expected_keys {
                    let generated_value = generated_json_object.get(key.as_str()).unwrap();
                    let expected_value = match key.as_str() {
                        k @ "provide" => {
                            if let Value::Object(obj) =
                                &mut expected_json_object.get_mut(k).unwrap()
                            {
                                obj.entry("heroku-sys/heroku").and_modify(|exp| {
                                    let gen = generated_value.get("heroku-sys/heroku").unwrap();
                                    if gen.as_str().unwrap().starts_with(exp.as_str().unwrap()) {
                                        *exp = gen.clone();
                                    }
                                });
                            }
                            expected_json_object.get(k).unwrap()
                        } // TODO: normalize "heroku-sys/heroku" stack version
                        k @ "repositories" => expected_json_object.get(k).unwrap(), // TODO: maybe normalize plugin repo path, maybe sort packages in "package" repo?
                        k @ _ => expected_json_object.get(k).unwrap(),
                    };

                    let comparison = assert_json_matches_no_panic(
                        generated_value,
                        expected_value,
                        Config::new(CompareMode::Strict),
                    )
                    .map_err(|err| {
                        format!("case {}, key {}: {}", case.name.as_ref().unwrap(), key, err)
                    });

                    assert!(comparison.is_ok(), "{}", comparison.unwrap_err());
                }
            })
    }

    // #[test]
    fn yo() {
        let composer_lock = r#"{
    "_readme": [
        "This file locks the dependencies of your project to a known state",
        "Read more about it at https://getcomposer.org/doc/01-basic-usage.md#installing-dependencies",
        "This file is @generated automatically"
    ],
    "content-hash": "c2b9dcae256d1b255b7265eef089f6c3",
    "packages": [
        {
            "name": "symfony/polyfill-php80",
            "version": "v1.23.1",
            "source": {
                "type": "git",
                "url": "https://github.com/symfony/polyfill-php80.git",
                "reference": "1100343ed1a92e3a38f9ae122fc0eb21602547be"
            },
            "dist": {
                "type": "zip",
                "url": "https://api.github.com/repos/symfony/polyfill-php80/zipball/1100343ed1a92e3a38f9ae122fc0eb21602547be",
                "reference": "1100343ed1a92e3a38f9ae122fc0eb21602547be",
                "shasum": ""
            },
            "require": {
                "php": ">=7.1"
            },
            "type": "library",
            "extra": {
                "branch-alias": {
                    "dev-main": "1.23-dev"
                },
                "thanks": {
                    "name": "symfony/polyfill",
                    "url": "https://github.com/symfony/polyfill"
                }
            },
            "autoload": {
                "psr-4": {
                    "Symfony\\Polyfill\\Php80\\": ""
                },
                "files": [
                    "bootstrap.php"
                ],
                "classmap": [
                    "Resources/stubs"
                ]
            },
            "notification-url": "https://packagist.org/downloads/",
            "license": [
                "MIT"
            ],
            "authors": [
                {
                    "name": "Ion Bazan",
                    "email": "ion.bazan@gmail.com"
                },
                {
                    "name": "Nicolas Grekas",
                    "email": "p@tchwork.com"
                },
                {
                    "name": "Symfony Community",
                    "homepage": "https://symfony.com/contributors"
                }
            ],
            "description": "Symfony polyfill backporting some PHP 8.0+ features to lower PHP versions",
            "homepage": "https://symfony.com",
            "keywords": [
                "compatibility",
                "polyfill",
                "portable",
                "shim"
            ],
            "support": {
                "source": "https://github.com/symfony/polyfill-php80/tree/v1.23.1"
            },
            "funding": [
                {
                    "url": "https://symfony.com/sponsor",
                    "type": "custom"
                },
                {
                    "url": "https://github.com/fabpot",
                    "type": "github"
                },
                {
                    "url": "https://tidelift.com/funding/github/packagist/symfony/symfony",
                    "type": "tidelift"
                }
            ],
            "time": "2021-07-28T13:41:28+00:00"
        },
        {
            "name": "symfony/process",
            "version": "v5.1.0-RC1",
            "source": {
                "type": "git",
                "url": "https://github.com/symfony/process.git",
                "reference": "14c0d48567aafd6b24001866de32ae45b0e3e1d1"
            },
            "dist": {
                "type": "zip",
                "url": "https://api.github.com/repos/symfony/process/zipball/14c0d48567aafd6b24001866de32ae45b0e3e1d1",
                "reference": "14c0d48567aafd6b24001866de32ae45b0e3e1d1",
                "shasum": ""
            },
            "require": {
                "php": "^7.2.5",
                "symfony/polyfill-php80": "^1.15"
            },
            "type": "library",
            "extra": {
                "branch-alias": {
                    "dev-master": "5.1-dev"
                }
            },
            "autoload": {
                "psr-4": {
                    "Symfony\\Component\\Process\\": ""
                },
                "exclude-from-classmap": [
                    "/Tests/"
                ]
            },
            "notification-url": "https://packagist.org/downloads/",
            "license": [
                "MIT"
            ],
            "authors": [
                {
                    "name": "Fabien Potencier",
                    "email": "fabien@symfony.com"
                },
                {
                    "name": "Symfony Community",
                    "homepage": "https://symfony.com/contributors"
                }
            ],
            "description": "Symfony Process Component",
            "homepage": "https://symfony.com",
            "support": {
                "source": "https://github.com/symfony/process/tree/master"
            },
            "funding": [
                {
                    "url": "https://symfony.com/sponsor",
                    "type": "custom"
                },
                {
                    "url": "https://github.com/fabpot",
                    "type": "github"
                },
                {
                    "url": "https://tidelift.com/funding/github/packagist/symfony/symfony",
                    "type": "tidelift"
                }
            ],
            "time": "2020-04-15T16:09:08+00:00"
        }
    ],
    "packages-dev": [
        {
            "name": "kahlan/kahlan",
            "version": "5.1.3",
            "source": {
                "type": "git",
                "url": "https://github.com/kahlan/kahlan.git",
                "reference": "bbf99064b7b78049f58e20138bee18fcdee3573e"
            },
            "dist": {
                "type": "zip",
                "url": "https://api.github.com/repos/kahlan/kahlan/zipball/bbf99064b7b78049f58e20138bee18fcdee3573e",
                "reference": "bbf99064b7b78049f58e20138bee18fcdee3573e",
                "shasum": ""
            },
            "require": {
                "php": ">=7.1"
            },
            "require-dev": {
                "squizlabs/php_codesniffer": "^3.4"
            },
            "bin": [
                "bin/kahlan"
            ],
            "type": "library",
            "autoload": {
                "psr-4": {
                    "Kahlan\\": "src/"
                },
                "files": [
                    "src/functions.php"
                ]
            },
            "notification-url": "https://packagist.org/downloads/",
            "license": [
                "MIT"
            ],
            "authors": [
                {
                    "name": "CrysaLEAD"
                }
            ],
            "description": "The PHP Test Framework for Freedom, Truth and Justice.",
            "keywords": [
                "BDD",
                "Behavior-Driven Development",
                "Monkey Patching",
                "TDD",
                "mock",
                "stub",
                "testing",
                "unit test"
            ],
            "support": {
                "issues": "https://github.com/kahlan/kahlan/issues",
                "source": "https://github.com/kahlan/kahlan/tree/5.1.3"
            },
            "time": "2021-06-13T11:14:50+00:00"
        }
    ],
    "aliases": [],
    "minimum-stability": "RC",
    "stability-flags": {
        "symfony/process": 5
    },    
    "prefer-stable": true,
    "prefer-lowest": false,
    "platform": {
        "ext-gmp": "*",
        "ext-intl": "*",
        "ext-mbstring": "*",
        "ext-redis": "*",
        "ext-sqlite3": "*",
        "ext-ldap": "*",
        "ext-imap": "*",
        "ext-blackfire": "*"
    },
    "platform-dev": {
        "ext-pcov": "*"
    },
    "plugin-api-version": "2.3.0"
}
"#;
        let l: ComposerLock = serde_json::from_str(composer_lock).unwrap();

        let stack = env::var("STACK").unwrap_or_else(|_| "heroku-22".to_string());

        // our default repo
        let default_repos = vec![Url::parse(
            format!(
                "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-cnb/",
                stack,
            )
            .as_str(),
        )
        .unwrap()];
        // anything user-supplied
        let byo_repos = env::var("HEROKU_PHP_PLATFORM_REPOSITORIES").unwrap_or_default();
        let all_repos = repos_from_defaults_and_list(&default_repos, &byo_repos).unwrap();

        let pj = serde_json::to_string_pretty(
            &generate_platform_json(
                &l,
                &stack,
                &PathBuf::from("../../support/installer"),
                &all_repos,
                env::var("HEROKU_PHP_INSTALL_DEV").is_ok(),
            )
            .unwrap()
            .0, // FIXME: handle
        )
        .unwrap();

        println!("{pj}");
    }

    // #[test]
    fn nothing() {
        let l = ComposerLock::new(Some("2.99.0".into()));

        let stack = env::var("STACK").unwrap_or_else(|_| "heroku-22".to_string());

        // our default repo
        let default_repos = vec![Url::parse(
            format!(
                "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-cnb/",
                stack,
            )
            .as_str(),
        )
        .unwrap()];

        let pj = serde_json::to_string_pretty(
            &generate_platform_json(
                &l,
                &stack,
                &PathBuf::from("../../support/installer"),
                &default_repos,
                false,
            )
            .unwrap()
            .0, // FIXME: handle
        )
        .unwrap();

        println!("{pj}");
    }
}
