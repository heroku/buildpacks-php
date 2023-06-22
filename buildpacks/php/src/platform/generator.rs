use crate::package_manager;
use crate::utils::regex;
use chrono::offset::Utc;
use composer::{
    ComposerBasePackage, ComposerLock, ComposerPackage, ComposerRepository,
    ComposerRepositoryFilters, ComposerRootPackage, ComposerStability,
};
use serde_json::json;
use std::collections::HashMap;
use std::path::Path;
use std::string::ToString;
use url::Url;

/// Adds the `heroku-sys/` package name prefix to the given input string, if not already present.
pub(crate) fn ensure_heroku_sys_prefix(name: impl AsRef<str>) -> String {
    let name = name.as_ref();
    format!(
        "heroku-sys/{}",
        name.strip_prefix("heroku-sys/").unwrap_or(name)
    )
}

/// Splits the given string by the given separator, and returns an iterator over the non-empty items, with whitespace trimmed.
fn split_and_trim_list<'a>(list: &'a str, sep: &'a str) -> impl Iterator<Item = &'a str> {
    list.split(sep)
        .map(str::trim)
        .filter_map(|p| (!p.is_empty()).then_some(p))
}

/// Parses a given repository [`Url`] with optional priority and filter query args into a [`ComposerRepository`].
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
                ));
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

    #[allow(clippy::default_trait_access)]
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
pub(crate) enum ComposerRepositoryFromRepositoryUrlError {
    MultipleFilters,
}

#[derive(Debug)]
pub(crate) enum PlatformGeneratorError {
    EmptyPlatformRepositoriesList,
    FromRepositoryUrl(ComposerRepositoryFromRepositoryUrlError),
    InvalidStackIdentifier(String),
}

/// Input data describing the desired packages and stabilities for [`generate_platform_json`]
#[derive(Default, Debug)]
pub(crate) struct PlatformJsonGeneratorInput {
    /// The desired [`ComposerStability`] for the root package's `minimum-stability` field
    pub minimum_stability: ComposerStability,
    /// The desired value for the root package's `prefer-stable` field
    pub prefer_stable: bool,
    /// The direct platform requirements from the root dependencies of the source project
    pub platform_require: HashMap<String, String>,
    /// The direct platform dev requirements from the root dependencies of the source project
    pub platform_require_dev: HashMap<String, String>,
    /// A list of packages from the source project's locked dependencies
    pub packages: Vec<ComposerPackage>,
    /// A list of packages from the source project's locked dev dependencies
    pub packages_dev: Vec<ComposerPackage>,
    /// A list of additional requirements to be placed into the generated package's root requirements
    pub additional_require: Option<HashMap<String, String>>,
    /// A list of additional requirements to be placed into the generated package's root dev requirements
    pub additional_require_dev: Option<HashMap<String, String>>,
    /// Additional [`ComposerRepository`] entries to be placed into the generated package
    pub additional_repositories: Option<Vec<ComposerRepository>>,
}
impl From<&ComposerLock> for PlatformJsonGeneratorInput {
    fn from(lock: &ComposerLock) -> Self {
        Self {
            minimum_stability: lock.minimum_stability.clone(),
            prefer_stable: lock.prefer_stable,
            platform_require: (*lock.platform).clone(),
            platform_require_dev: (*lock.platform_dev).clone(),
            packages: lock.packages.clone(),
            packages_dev: lock.packages_dev.clone(),
            additional_require: None,
            additional_require_dev: None,
            additional_repositories: None,
        }
    }
}

fn stack_provide_from_stack_name(stack: &str) -> Result<(String, String), PlatformGeneratorError> {
    // from the given stack string like "heroku-99", make a ("heroku-sys/heroku", "99.2023.04.05") tuple for "provide" later
    let stack_captures = regex!(r"^(?P<stackname>[^-]+)(?:-(?P<stackversion>[0-9]+))?$")
        .captures(stack)
        .ok_or(PlatformGeneratorError::InvalidStackIdentifier(
            stack.to_string(),
        ))?;
    Ok((
        ensure_heroku_sys_prefix(
            stack_captures
                .name("stackname")
                .ok_or(PlatformGeneratorError::InvalidStackIdentifier(
                    stack.to_string(),
                ))?
                .as_str(),
        ),
        format!(
            "{}.{}",
            stack_captures
                .name("stackversion")
                .map_or("1", |m| m.as_str()),
            Utc::now().format("%Y.%0m.%0d")
        ),
    ))
}

/// Generates a [`ComposerRootPackage`] for the given:
/// - [`PlatformJsonGeneratorInput`],
/// - stack name,
/// - path to the Composer installer plugin, and
/// - list of platform repository URLs.
///
/// A "provide" entry on the root package is automatically generated for the given stack.
///
/// From the given platform repository URLs, a "composer" type repository entry is generated for each.
/// The repositories are inserted in reverse order to allow later repositories to override packages from earlier ones.
/// For details on this (and Composer's) repository precedence behavior, and how to control it via URL query args, see [`composer_repository_from_repository_url`]
pub(crate) fn generate_platform_json(
    input: &PlatformJsonGeneratorInput,
    stack: &str,
    installer_path: &Path,
    platform_repositories: &Vec<Url>,
) -> Result<ComposerRootPackage, PlatformGeneratorError> {
    if platform_repositories.is_empty() {
        return Err(PlatformGeneratorError::EmptyPlatformRepositoriesList);
    };

    let stack_provide = stack_provide_from_stack_name(stack)?;

    // some fundamental stuff we want installed
    let mut require = HashMap::from([
        // our installer plugin - individual platform packages are also supposed to require it, but hey
        ("heroku/installer-plugin".to_string(), "*".to_string()),
    ]);
    let mut require_dev: HashMap<String, String> = HashMap::new();

    // disable packagist.org (we want to userland package installs here), and add the installer plugin
    let mut repositories = vec![
        serde_json::from_value(json!({"packagist.org": false}))
            .expect("Internal error: repository construction via serde_json"),
        // our heroku/installer-plugin
        ComposerRepository::from_path_with_options(
            installer_path,
            json!({"symlink": false}).as_object().cloned(),
        ),
    ];
    // additional repositories come next; this could be e.g. path or package repos for packages in .additional_require
    repositories.append(
        input
            .additional_repositories
            .clone()
            .unwrap_or_default()
            .as_mut(),
    );

    // process the given platform repository URLs and insert them into the list
    repositories.append(
        platform_repositories
            .iter()
            .map(|url| {
                composer_repository_from_repository_url(url.clone())
                    .map_err(PlatformGeneratorError::FromRepositoryUrl)
            })
            // repositories are passed in in ascending order of precedence
            // typically our default repo first, then user-supplied repos after that
            // by default, repositories are canonical, so lookups will not happen in later repos if a package is found in an earlier repo
            // (even if the later repo has newer, or better matching for other requirements, versions)
            // so we reverse the list to allow later repositories to overwrite packages from earlier ones
            // users can still have Composer fall back to e.g. the default repo for newer versions using ?composer-repository-canonical=false
            .rev()
            .collect::<Result<Vec<ComposerRepository>, _>>()?
            .as_mut(),
    );

    // we take all requirements from platform(-dev) and carry them over (with a "heroku-sys/" prefix) into our root require(-dev)
    // ^ these also already contain the correct original stability flags, if any (think "php: 8.4.0@RC"), in their version strings
    // ^ convenient for us, because stability flags are ignored in requirements outside the root package's require section
    // ^ this is done intentionally by Composer to prevent dependencies from pushing unstable stuff onto users without explicit opt-in
    //
    // for all packages(-dev), i.e. the userland packages solved into the lock file, create a type=metapackage
    // ^ with name and version copied
    // ^ with require, replace, provide and conflict entries that reference a platform package, again with a "heroku-sys/" prefix
    // ^ these metapackages are inserted together as a "package" type repository, and a require(-dev) entry is written for each
    //
    // regardless of dev install or not, we process all platform-dev and packages-dev packages so the caller can tell later if there is no version requirement in all of require, but in require-dev
    // ^ this might be desired to ensure folks get the same PHP version etc as locally/CI
    // ^ it is then up to the caller to not write the collected requirements into require-dev in the case of a non-dev install (by removing the requirements)
    // ^ this will be necessary because "composer update" has to check "require-dev" packages to ensure lock file consistency even if "--no-dev" is used
    //   (--no-dev only affects dependency installation, not overall dependency resolution)
    //   (and people frequently have e.g. ext-xdebug as a dev requirement)

    for (platform, packages, requires) in [
        (&input.platform_require, &input.packages, &mut require),
        (
            &input.platform_require_dev,
            &input.packages_dev,
            &mut require_dev,
        ),
    ] {
        // first, the root platform requirements from "platform"/"platform-dev" are simply carried over
        requires.extend(
            platform
                .clone()
                .into_iter()
                // each requirement name gets the expected "heroku-sys/" prefix
                .map(|(name, version)| (ensure_heroku_sys_prefix(name), version)),
        );

        // then, we build metapackages that "mimic" all regular "packages"/"packages-dev" entries...
        let mut metapackages = packages
            .iter()
            // ... but with only their platform ("php", "ext-foobar", ...) links (require/provide/conflict/replace) included (and prefixed with "heroku-sys/")
            .filter_map(package_manager::composer::package_with_only_platform_links)
            .peekable();

        // if that even resulted in any packages (we also filtered out any packages without any platform links)
        if metapackages.peek().is_some() {
            // put all packages into a ComposerPackageRepository...
            repositories.push(
                metapackages
                    // ... and insert a require for each of them
                    .map(|package| {
                        requires.insert(package.name.clone(), package.version.clone());
                        package
                    })
                    .collect(),
            );
        }
    }

    // add explicit additional requirements from config
    // we do this last to allow caller to override anything we computed or generated
    require.extend(input.additional_require.clone().unwrap_or_default());
    require_dev.extend(input.additional_require_dev.clone().unwrap_or_default());

    Ok(ComposerRootPackage {
        config: json!({
            "cache-files-ttl": 0,
            "discard-changes": true,
            "allow-plugins": {
                "heroku/installer-plugin": true,
            }
        })
        .as_object()
        .cloned(),
        minimum_stability: Some(input.minimum_stability.clone()),
        prefer_stable: Some(input.prefer_stable),
        package: ComposerBasePackage {
            provide: Some(HashMap::from([stack_provide])),
            replace: None, // TODO: blackfire
            repositories: Some(repositories),
            require: (!require.is_empty()).then_some(require),
            require_dev: (!require_dev.is_empty()).then_some(require_dev),
            ..Default::default()
        },
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::package_manager::composer::PlatformExtractorNotice;

    use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};
    use figment::providers::{Format, Serialized, Toml};
    use figment::{value::magic::RelativePathBuf, Figment};
    use serde::{Deserialize, Serialize};
    use serde_json::{Map, Value};
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::{env, fs};

    #[derive(Deserialize, Serialize)]
    struct ComposerLockTestCaseConfig {
        name: Option<String>,
        description: Option<String>,
        lock: Option<RelativePathBuf>,
        stack: String,
        expected_result: Option<RelativePathBuf>,
        expected_extractor_notices: Option<Vec<String>>, // TODO: can we use PlatformExtractorNotice?
        expected_finalizer_notices: Option<Vec<String>>, // TODO: can we use PlatformFinalizerNotice?
        expect_extractor_failure: Option<String>,        // TODO: can we use PlatformExtractorError?
        expect_generator_failure: Option<String>,        // TODO: can we use PlatformGeneratorError?
        expect_finalizer_failure: Option<String>,        // TODO: can we use PlatformFinalizerError?
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
                repositories: vec![Url::parse(&format!(
                    "https://lang-php.s3.us-east-1.amazonaws.com/dist-{stack}-cnb/packages.json",
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

        fs::read_dir(Path::new("tests/fixtures/platform/generator"))
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

                // FIRST: from the lock file, extract a generator config and packages list

                let generator_input = package_manager::composer::extract_from_lock(&lock);

                // first check: was this even supposed to succeed or fail?
                assert_eq!(
                    generator_input.is_ok(),
                    case.expect_extractor_failure.is_none(),
                    "case {}: lock extraction expected to {}, but it didn't",
                    case.name.as_ref().unwrap(),
                    if generator_input
                        .is_ok() { "fail" } else { "succeed" },
                );

                // on failure, check if the type of failure what was the test expected
                let mut extractor_notices  = Vec::<PlatformExtractorNotice>::new();
                let generator_input  = match generator_input
                {
                    Ok(v) => v.unwrap(&mut extractor_notices),
                    Err(e) => {
                        assert!(
                            case.expect_extractor_failure.is_some(),
                            "case {}: lock extraction failed, but config has no expect_extractor_failure type specified",
                            case.name.as_ref().unwrap()
                        );

                        assert_eq!(
                            format!("{e:?}"),
                            case.expect_extractor_failure.unwrap(),
                            "case {}: lock extraction failed as expected, but with mismatched failure type",
                            case.name.as_ref().unwrap()
                        );

                        return;
                    }
                };

                // fetch all notices and compare them against the list of expected notices
                assert_eq!(
                    extractor_notices
                        .iter()
                        .map(|v| format!("{v:?}"))
                        .collect::<HashSet<String>>(),
                    case.expected_extractor_notices
                        .unwrap_or_default()
                        .into_iter()
                        .collect::<HashSet<String>>(),
                    "case {}: mismatched lock extractor notices (left = generated, right = expected)",
                    case.name.as_ref().unwrap()
                );

                // SECOND: generate "platform.json" from the extracted config and packages list

                let generated_json_package = generate_platform_json(
                    &generator_input,
                    &case.stack,
                    installer_path,
                    &case.repositories,
                );

                // first check: was this even supposed to succeed or fail?
                assert_eq!(
                    generated_json_package.is_ok(),
                    case.expect_generator_failure.is_none(),
                    "case {}: generation expected to {}, but it didn't",
                    case.name.as_ref().unwrap(),
                    if generated_json_package
                        .is_ok() { "fail" } else { "succeed" },
                );

                // on failure, check if the type of failure what was the test expected
                let mut generated_json_package = match generated_json_package
                {
                    Ok(v) => v,
                    Err(e) => {
                        assert!(
                            case.expect_generator_failure.is_some(),
                            "case {}: generation failed, but config has no expect_generator_failure type specified",
                            case.name.as_ref().unwrap()
                        );

                        assert_eq!(
                            format!("{e:?}"),
                            case.expect_generator_failure.unwrap(),
                            "case {}: generation failed as expected, but with mismatched failure type",
                            case.name.as_ref().unwrap()
                        );

                        return;
                    }
                };

                // THIRD: post-process the generated result to ensure/validate runtime requirements etc
                let ensure_runtime_requirement_result = package_manager::composer::ensure_runtime_requirement(&mut generated_json_package);

                // first check: was this even supposed to succeed or fail?
                assert_eq!(
                    ensure_runtime_requirement_result.is_ok(),
                    case.expect_finalizer_failure.is_none(),
                    "case {}: finalizing expected to {}, but it didn't",
                    case.name.as_ref().unwrap(),
                    if ensure_runtime_requirement_result
                        .is_ok() { "fail" } else { "succeed" },
                );

                // on failure, check if the type of failure what was the test expected
                let finalizer_notices = match ensure_runtime_requirement_result
                {
                    Ok(v) => v,
                    Err(e) => {
                        assert!(
                            case.expect_finalizer_failure.is_some(),
                            "case {}: finalizing failed, but config has no expect_finalizer_failure type specified",
                            case.name.as_ref().unwrap()
                        );

                        assert_eq!(
                            format!("{e:?}"),
                            case.expect_finalizer_failure.unwrap(),
                            "case {}: finalizing failed as expected, but with mismatched failure type",
                            case.name.as_ref().unwrap()
                        );

                        return;
                    }
                };

                // fetch all notices and compare them against the list of expected notices
                assert_eq!(
                    finalizer_notices
                        .iter()
                        .map(|v| format!("{v:?}"))
                        .collect::<HashSet<String>>(),
                    case.expected_finalizer_notices
                        .unwrap_or_default()
                        .into_iter()
                        .collect::<HashSet<String>>(),
                    "case {}: mismatched finalizer notices (left = generated, right = expected)",
                    case.name.as_ref().unwrap()
                );

                if !case.install_dev {
                    // remove require-dev if we do not want dev installs
                    generated_json_package.package.require_dev.take();
                }

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
                                // for heroku-sys/heroku, we want to check that the generated value starts with the expected value
                                // (since the version strings are like XX.YYYY.MM.DD, with XX being the stack version number)
                                obj.entry("heroku-sys/heroku").and_modify(|exp| {
                                    let gen = generated_value.get("heroku-sys/heroku").unwrap();
                                    if gen.as_str().unwrap().starts_with(exp.as_str().unwrap()) {
                                        *exp = gen.clone();
                                    }
                                });
                            }
                            expected_json_object.get(k).unwrap()
                        }
                        // k @ "repositories" => expected_json_object.get(k).unwrap(), // maybe normalize plugin repo path, maybe sort packages in "package" repo?
                        k => expected_json_object.get(k).unwrap(),
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
            });
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
            format!("https://lang-php.s3.us-east-1.amazonaws.com/dist-{stack}-cnb/",).as_str(),
        )
        .unwrap()];
        // anything user-supplied
        let byo_repos = env::var("HEROKU_PHP_PLATFORM_REPOSITORIES").unwrap_or_default();
        let all_repos = crate::platform::platform_repository_urls_from_defaults_and_list(
            &default_repos,
            byo_repos,
        )
        .unwrap();

        let generator_input = package_manager::composer::extract_from_lock(&l)
            .unwrap()
            .value;

        let pj = serde_json::to_string_pretty(
            &generate_platform_json(
                &generator_input,
                &stack,
                &PathBuf::from("../../support/installer"),
                &all_repos,
            )
            .unwrap(), // FIXME: handle
        )
        .unwrap();

        println!("{pj}");
    }

    // #[test]
    fn nothing() {
        let stack = env::var("STACK").unwrap_or_else(|_| "heroku-22".to_string());

        // our default repo
        let default_repos = vec![Url::parse(
            format!("https://lang-php.s3.us-east-1.amazonaws.com/dist-{stack}-cnb/",).as_str(),
        )
        .unwrap()];

        let generator_input = PlatformJsonGeneratorInput {
            additional_require: Some(HashMap::from([
                ("heroku-sys/composer".to_string(), "*".to_string()),
                ("heroku-sys/php".to_string(), "*".to_string()),
            ])),
            ..Default::default()
        };

        let mut pj = generate_platform_json(
            &generator_input,
            &stack,
            &PathBuf::from("../../support/installer"),
            &default_repos,
        )
        .unwrap();

        // remove require-dev if we do not want dev installs
        pj.package.require_dev.take();

        let pj = serde_json::to_string_pretty(&pj).unwrap();

        println!("{pj}");
    }
}
