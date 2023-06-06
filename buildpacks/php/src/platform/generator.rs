use std::collections::{HashMap, HashSet};
use std::ops::Not;
use std::path::Path;
use std::string::ToString;

use crate::package_manager::composer::{PlatformExtractorError, PlatformExtractorNotice};
use chrono::offset::Utc;
use composer::{
    ComposerBasePackage, ComposerLock, ComposerPackage, ComposerRepository,
    ComposerRepositoryFilters, ComposerRootPackage, ComposerStability,
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

// TODO refactor: move to package_manager/composer ???
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

// TODO refactor: move to package_manager/composer
fn has_runtime_requirement(requires: &HashMap<String, String>) -> bool {
    requires.contains_key("heroku-sys/php")
}

// TODO refactor: move to package_manager/composer ???
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
}
// TODO refactor: move to package_manager/composer
#[derive(strum_macros::Display, Debug, Eq, PartialEq)]
pub(crate) enum PlatformFinalizerError {
    RuntimeRequirementInRequireDevButNotRequire,
}
// TODO refactor: move to package_manager/composer
#[derive(strum_macros::Display, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PlatformFinalizerNotice {
    RuntimeRequirementInserted(String, String),
    RuntimeRequirementFromDependencies,
}

// TODO refactor: move to package_manager/composer ???
fn package_with_only_platform_links(package: &ComposerPackage) -> Option<ComposerPackage> {
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
}

// TODO refactor: move to package_manager/composer
fn process_composer_version(
    lock: &ComposerLock,
) -> Result<(HashMap<String, String>, HashSet<PlatformExtractorNotice>), PlatformExtractorError> {
    let mut notices = HashSet::new();
    let mut requires = HashMap::new();
    // we want the latest Composer...
    requires.insert(
        ensure_heroku_sys_prefix("composer").to_string(),
        "*".to_string(),
    );
    // ... that supports the major plugin API version from the lock file (which corresponds to the Composer version series, so e.g. all 2.3.x releases have 2.3.0)
    // if the lock file says "2.99.0", we generally still want to select "^2", and not "^2.99.0"
    // this is so the currently available Composer version can install lock files generated by brand new or pre-release Composer versions, as this stuff is generally forward compatible
    // otherwise, builds would fail the moment e.g. 2.6.0 comes out and people try it, even though 2.5 could install the project just fine
    requires.insert(
        ensure_heroku_sys_prefix("composer-plugin-api").to_string(),
        match lock.plugin_api_version.as_deref() {
            // no rule without an exception, of course:
            // there are quite a lot of BC breaks for plugins in Composer 2.3
            // if the lock file was generated with 2.0, 2.1 or 2.2, we play it safe and install 2.2.x (which is LTS)
            // this is mostly to ensure any plugins that have an open enough version selector do not break with all the 2.3 changes
            // also ensures plugins are compatible with other libraries Composer bundles (e.g. various Symfony components), as those got big version bumps in 2.3
            Some(v @ ("2.0.0" | "2.1.0" | "2.2.0")) => {
                let r = "~2.2.0".to_string();
                notices.insert(PlatformExtractorNotice::ComposerPluginApiVersionConfined(
                    v.to_string(),
                    r.clone(),
                ));
                r
            }
            // just "^2" or similar so we get the latest we have, see comment earlier
            Some(v) => format!(
                "^{}",
                v.split_once('.')
                    .ok_or(PlatformExtractorError::InvalidPlatformApiVersion)?
                    .0
            ),
            // nothing means it's pre-v1.10, in which case we want to just use v1
            None => {
                let r = "^1.0.0".to_string();
                notices.insert(PlatformExtractorNotice::NoComposerPluginApiVersionInLock(
                    r.clone(),
                ));
                r
            }
        },
    );
    Ok((requires, notices))
}

// TODO refactor: move to package_manager/composer
fn extract_root_requirements(
    platform: &HashMap<String, String>,
    generated_package_name: String,
    generated_package_version: String,
) -> Option<ComposerPackage> {
    let root_platform_requires = extract_platform_links_with_heroku_sys(platform)?;

    Some(ComposerPackage {
        name: generated_package_name,
        version: generated_package_version,
        package: ComposerBasePackage {
            kind: Some("metapackage".into()),
            require: Some(root_platform_requires),
            ..Default::default()
        },
        ..Default::default()
    })
}

// TODO refactor: move to package_manager/composer
pub(crate) fn extract_from_lock(
    lock: &ComposerLock,
) -> Result<(PlatformJsonGeneratorInput, HashSet<PlatformExtractorNotice>), PlatformExtractorError>
{
    let mut config = PlatformJsonGeneratorInput::from(lock);
    let composer_requires = process_composer_version(lock)?;

    config.additional_require.replace(composer_requires.0);

    Ok((config, composer_requires.1))
}

// TODO refactor: move to package_manager/composer
pub(crate) fn ensure_runtime_requirement(
    root_package: &mut ComposerRootPackage,
) -> Result<HashSet<PlatformFinalizerNotice>, PlatformFinalizerError> {
    let mut notices = HashSet::new();

    // from all our metapackages, dev or not, make a lookup table
    let metapaks = root_package
        .package
        .repositories
        .clone()
        .unwrap_or_default()
        .into_iter()
        .filter(|repo| matches!(repo, ComposerRepository::Package { .. }))
        .fold(HashMap::new(), |mut acc, repo| match repo {
            ComposerRepository::Package { package, .. } => {
                acc.extend(
                    package
                        .into_iter()
                        .map(|package| (package.name.clone(), package)),
                );
                acc
            }
            _ => acc,
        });

    // is there a requirement for php in the root?
    if !has_runtime_requirement(&root_package.package.require.clone().unwrap_or_default()) {
        // there is not!
        let mut seen_runtime_requirement = false;
        let mut seen_runtime_dev_requirement = false;

        // see if any of the metapackages listed in require/require-dev has one
        for (marker, requirements) in [
            // process require and require-dev separately
            (&mut seen_runtime_requirement, &root_package.package.require),
            (
                &mut seen_runtime_dev_requirement,
                &root_package.package.require_dev,
            ),
        ] {
            for (name, _) in requirements.clone().unwrap_or_default() {
                *marker |= metapaks.get(&name).map_or(false, |package| {
                    // here, we only look at a package's require list, not require-dev, which only has an effect in the root of a composer.json
                    // (since every library has its own list of dev requirements for testing etc, and that should never be installed into a project using that library)
                    has_runtime_requirement(&package.package.require.clone().unwrap_or_default())
                });
            }
        }

        if seen_runtime_requirement {
            // some dependenc(y|ies) required a runtime, which will be used
            notices.insert(PlatformFinalizerNotice::RuntimeRequirementFromDependencies);
        } else if seen_runtime_dev_requirement {
            // no runtime requirement anywhere, but there is a requirement in a require-dev package, which we do not allow
            return Err(PlatformFinalizerError::RuntimeRequirementInRequireDevButNotRequire);
        } else {
            // nothing anywhere; time to insert a default!
            let name = "php".to_string();
            let version = "*".to_string();
            notices.insert(PlatformFinalizerNotice::RuntimeRequirementInserted(
                name.clone(),
                version.clone(),
            ));
            // TODO: if we have seen `php-…` variants like `php-ipv6` or `php-zts`, should we warn or fail?
            root_package
                .package
                .require
                .get_or_insert(HashMap::new()) // could be None
                .insert(ensure_heroku_sys_prefix(name), version);
        }
    }

    // TODO: generate conflict entries for php-zts, php-debug?

    Ok(notices)
}

#[derive(Default, Debug)]
pub(crate) struct PlatformJsonGeneratorInput {
    pub input_name: String,
    pub input_revision: String,
    pub minimum_stability: ComposerStability,
    pub prefer_stable: bool,
    pub platform_require: HashMap<String, String>,
    pub platform_require_dev: HashMap<String, String>,
    pub packages: Vec<ComposerPackage>,
    pub packages_dev: Vec<ComposerPackage>,
    pub additional_require: Option<HashMap<String, String>>,
    pub additional_require_dev: Option<HashMap<String, String>>,
}
// TODO refactor: move to package_manager/composer
impl From<&ComposerLock> for PlatformJsonGeneratorInput {
    fn from(lock: &ComposerLock) -> Self {
        Self {
            input_name: "composer.json/composer.lock".to_string(),
            input_revision: lock.content_hash.clone(),
            minimum_stability: lock.minimum_stability.clone(),
            prefer_stable: lock.prefer_stable.clone(),
            platform_require: (*lock.platform).clone(),
            platform_require_dev: (*lock.platform_dev).clone(),
            packages: lock.packages.clone(),
            packages_dev: lock.packages_dev.clone(),
            additional_require: None,
            additional_require_dev: None,
        }
    }
}

pub(crate) fn generate_platform_json(
    input: PlatformJsonGeneratorInput,
    stack: &str,
    installer_path: &Path,
    platform_repositories: &Vec<Url>,
) -> Result<ComposerRootPackage, PlatformGeneratorError> {
    if platform_repositories.is_empty() {
        return Err(PlatformGeneratorError::EmptyPlatformRepositoriesList);
    };

    // from the given stack string like "heroku-99", make a ("heroku-sys/heroku", "99.2023.04.05") tuple for "provide" later
    let stack_captures = Regex::new(r"^([^-]+)(?:-([0-9]+))?$")
        .expect("A certain somebody broke the stack parsing regex. Yes, I am looking at you.")
        .captures(stack)
        .ok_or(PlatformGeneratorError::InvalidStackIdentifier)?;
    let stack_provide = (
        ensure_heroku_sys_prefix(String::from(stack_captures.get(1).unwrap().as_str())),
        format!(
            "{}.{}",
            stack_captures.get(2).map_or("1", |m| m.as_str()),
            Utc::now().format("%Y.%0m.%0d")
        ),
    );

    // some fundamental stuff we want installed
    let mut require: HashMap<String, String> = [
        // our installer plugin - individual platform packages are also supposed to require it, but hey
        ("heroku/installer-plugin", "*"),
    ]
    .iter()
    .map(|v| (v.0.into(), v.1.into()))
    .collect();
    let mut require_dev: HashMap<String, String> = HashMap::new();

    // disable packagist.org (we want to userland package installs here), and add the installer plugin
    let mut repositories = vec![
        ComposerRepository::Disabled(HashMap::from([("packagist.org".into(), MustBe!(false))])),
        // our heroku/installer-plugin
        ComposerRepository::from_path_with_options(
            installer_path,
            [("symlink".into(), Value::Bool(false))],
        ),
    ];

    // TODO refactor: this could be a separate install in a dedicated layer
    // web servers
    require.insert("heroku-sys/apache".to_string(), "^2.4.10".to_string());
    require.insert("heroku-sys/nginx".to_string(), "^1.8.0".to_string());
    // for now, we need the web server boot scripts and configs from the classic buildpack
    // so we install it as a package from a relative location - it's "above" the installer path
    match ComposerRepository::from_path_with_options(
        installer_path.join("../.."),
        [
            ("symlink".into(), Value::Bool(false)),
            (
                "versions".to_string(),
                json!({"heroku/heroku-buildpack-php": "dev-master"}),
            ),
        ],
    ) {
        r @ ComposerRepository::Path { .. } => {
            repositories.push(r);
            require.insert(
                "heroku/heroku-buildpack-php".to_string(),
                "dev-master".into(),
            );
        }
        _ => unreachable!(),
    };
    // TODO refactor: end

    // process the given platform repository URLs and insert them into the list
    repositories.append(
        platform_repositories
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
            .collect::<Result<Vec<ComposerRepository>, _>>()?
            .as_mut(),
    );

    // we take all requirements from platform(-dev) and move them into a special metapackage (named e.g. "composer.json/composer.lock")
    // ^ this package gets those platform requirements as its "require"s, with a "heroku-sys/" prefix, of course
    // ^ this is done because if the root package requires an extension, and there is also a dependency on a polyfill for it,
    //   e.g. "heroku-sys/ext-mbstring" and "symfony/polyfill-mbstring" (which declares "ext-mbstring" as "provide"d),
    //   there would be no way to know that anything required "ext-mbstring", since the solver optimizes this away,
    //   and no install event is fired for the root package itself
    // ^ we do however need to know this, since we have to attempt an install of the "real", "native" extension later
    // > the solution is to wrap the platform requirements into a metapackage, for which an install event fires, where we can extract these requirements
    //
    // we also copy all requirements from platform(-dev) into the list of root requirements
    // ^ we do that because otherwise, the original require of the package from the root would only be in that "composer.json/composer.lock" package mentioned earlier,
    //   but stability flags are ignored in requirements that are not in the root package's require section - think "php: 8.4.0@RC" in the root require section
    // ^ this is done intentionally by Composer to prevent dependencies from pushing unstable stuff onto users without explicit opt-in
    //
    // for all packages(-dev), create a type=metapackage for the package repo
    // ^ with name and version copied
    // ^ with require, replace, provide and conflict entries that reference a platform package rewritten with heroku-sys/ prefix
    //
    // regardless of dev install or not, we process all platform-dev and packages-dev packages so the caller can tell later if there is no version requirement in all of require, but in require-dev
    // ^ this might be desired to ensure folks get the same PHP version etc as locally/CI
    // ^ it is then up to the caller to not write the collected requirements into require-dev in the case of a non-dev install (by removing the requirements)
    // ^ this will be necessary because "composer update" has to check "require-dev" packages to ensure lock file consistency even if "--no-dev" is used
    //   (--no-dev only affects dependency installation, not overall dependency resolution)
    //   (and people frequently have e.g. ext-xdebug as a dev requirement)

    for (is_dev, platform, packages, requires) in [
        (
            false,
            &input.platform_require,
            &input.packages,
            &mut require,
        ),
        (
            true,
            &input.platform_require_dev,
            &input.packages_dev,
            &mut require_dev,
        ),
    ] {
        let mut metapackages = Vec::new();

        // first, for the root platform requires from "platform"/"platform-dev", make a special package
        // TODO: this can go if we manage to change the installer plugin to process the root package's requirements
        if let Some(root_package) = extract_root_requirements(
            platform,
            format!(
                "{}{}",
                &input.input_name,
                is_dev.then_some("-require-dev").unwrap_or_default() // different names for require and require-dev
            ),
            format!("dev-{}", &input.input_revision),
        ) {
            // we verbatim copy over the requirements from the root
            // this will take care of any stability flags for us without having to process them
            // it also makes it easier for code later to figure out if there was a requirement for something in the root or not
            // note:
            // key "require" on the package is correct here regardless of whether we're handling "platform" or "platform-dev":
            // the package we're creating here gets added to require or require-dev, and we're adding the packages to "require" or "require-dev",
            // but the actual list taken from "platform" or "platform-dev" is always put into "require" by extract_root_requirements() since we do not want Composer to ignore it
            requires.extend(root_package.package.require.clone().unwrap_or_default());
            // add this package to the list for the metapackage repository we're building
            metapackages.push(root_package);
        }

        // then, build packages for all regular requires
        metapackages.extend(packages.iter().filter_map(package_with_only_platform_links));

        if metapackages.is_empty().not() {
            for package in &metapackages {
                // now insert a dependency into the root for each require
                requires.insert(package.name.clone(), package.version.clone());
            }
            // put all packages into a ComposerPackageRepository
            repositories.push(metapackages.into());
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
        prefer_stable: Some(input.prefer_stable.clone()),
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

                // FIRST: from the lock file, extract a generator config and packages list

                let generator_input = extract_from_lock(&lock);

                // first check: was this even supposed to succeed or fail?
                assert_eq!(
                    generator_input.is_ok(),
                    case.expect_extractor_failure.is_none(),
                    "case {}: lock extraction expected to {}, but it didn't",
                    case.name.as_ref().unwrap(),
                    generator_input
                        .is_ok()
                        .then_some("fail")
                        .unwrap_or("succeed"),
                );

                // on failure, check if the type of failure what was the test expected
                let (generator_input, extractor_notices) = match generator_input
                {
                    Ok(v) => v,
                    Err(e) => {
                        assert!(
                            case.expect_extractor_failure.is_some(),
                            "case {}: lock extraction failed, but config has no expect_extractor_failure type specified",
                            case.name.as_ref().unwrap()
                        );

                        assert_eq!(
                            e.to_string(),
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
                        .map(|v| v.to_string())
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
                    generator_input,
                    &case.stack,
                    &installer_path,
                    &case.repositories,
                );

                // first check: was this even supposed to succeed or fail?
                assert_eq!(
                    generated_json_package.is_ok(),
                    case.expect_generator_failure.is_none(),
                    "case {}: generation expected to {}, but it didn't",
                    case.name.as_ref().unwrap(),
                    generated_json_package
                        .is_ok()
                        .then_some("fail")
                        .unwrap_or("succeed"),
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
                            e.to_string(),
                            case.expect_generator_failure.unwrap(),
                            "case {}: generation failed as expected, but with mismatched failure type",
                            case.name.as_ref().unwrap()
                        );

                        return;
                    }
                };

                // THIRD: post-process the generated result to ensure/validate runtime requirements etc
                let ensure_runtime_requirement_result = ensure_runtime_requirement(&mut generated_json_package);

                // first check: was this even supposed to succeed or fail?
                assert_eq!(
                    ensure_runtime_requirement_result.is_ok(),
                    case.expect_finalizer_failure.is_none(),
                    "case {}: finalizing expected to {}, but it didn't",
                    case.name.as_ref().unwrap(),
                    ensure_runtime_requirement_result
                        .is_ok()
                        .then_some("fail")
                        .unwrap_or("succeed"),
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
                            e.to_string(),
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
                        .map(|v| v.to_string())
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

        let (generator_input, _) = extract_from_lock(&l).unwrap();

        let pj = serde_json::to_string_pretty(
            &generate_platform_json(
                generator_input,
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
            format!(
                "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-cnb/",
                stack,
            )
            .as_str(),
        )
        .unwrap()];

        let generator_input = PlatformJsonGeneratorInput {
            input_name: "foo".to_string(),
            input_revision: "foo".to_string(),
            additional_require: Some(HashMap::from([
                ("heroku-sys/composer".to_string(), "*".to_string()),
                ("heroku-sys/php".to_string(), "*".to_string()),
            ])),
            ..Default::default()
        };

        let mut pj = generate_platform_json(
            generator_input,
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
