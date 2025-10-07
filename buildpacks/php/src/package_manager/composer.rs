use crate::platform::generator;
use crate::platform::generator::PlatformJsonGeneratorInput;
use crate::utils::regex;
use bullet_stream::global::print;
use composer::{
    ComposerBasePackage, ComposerLock, ComposerPackage, ComposerRepository, ComposerRootPackage,
};
use fun_run::CmdError;
use libcnb::Env;
use std::collections::HashMap;
use std::ops::Not;
use std::path::PathBuf;
use std::process::Command;
use warned::Warned;

#[derive(Debug)]
pub(crate) enum DependencyInstallationError {
    ComposerInstall(CmdError),
}

pub(crate) fn install_dependencies(
    dir: &PathBuf,
    command_env: &Env,
) -> Result<(), DependencyInstallationError> {
    print::sub_stream_cmd(
        Command::new("composer")
            .current_dir(dir)
            .envs(command_env)
            .args([
                "install",
                "--no-dev",
                "--no-progress",
                "--no-interaction",
                "--optimize-autoloader",
                "--prefer-dist",
            ]), // .envs(
                //     &[&platform_layer.env, &composer_env_layer.env]
                //         .iter()
                //         .fold(libcnb::Env::from_current(), |final_env, layer_env| {
                //             layer_env.apply(Scope::Build, &final_env)
                //         }),
                // ),
    )
    .map_err(DependencyInstallationError::ComposerInstall)?;

    // TODO: run `composer compile`? but is that still a good name?

    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PlatformExtractorError {
    ComposerLockVersion(ComposerLockVersionError),
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PlatformExtractorNotice {
    ComposerLockVersion(ComposerLockVersionNotice),
}

/// Checks whether the given package name represents what Composer refers to as a "platform package".
fn is_platform_package(name: impl AsRef<str>) -> bool {
    let name = name.as_ref();
    // same regex used by Composer as well
    regex!(r"^(?i)(?:php(?:-64bit|-ipv6|-zts|-debug)?|hhvm|(?:ext|lib)-[a-z0-9](?:[_.-]?[a-z0-9]+)*|composer(?:-(?:plugin|runtime)-api)?)$")
        .is_match(name)
        // ext-….native packages are ours, and ours alone - virtual packages to later force installation of native extensions in case of userland "provide"s
        && !(name.starts_with("ext-") && name.ends_with(".native"))
        // we ignore those for the moment - they're not in package metadata (yet), and if they were, the versions are "frozen" at build time, but stack images get updates...
        && !name.starts_with("lib-")
        // not currently in package metadata
        // TODO: put into package metadata so it's usable
        && name != "composer-runtime-api"
}

/// Checks whether the given list of package links (typically from "require") contains a requirement for a language runtime.
fn has_runtime_link(links: &HashMap<String, String>) -> bool {
    links.contains_key("heroku-sys/php")
}

/// Extracts links to platform packages (see [`is_platform_package`]) and prefix them using [`ensure_heroku_sys_prefix`].
fn extract_platform_links_with_heroku_sys<T: Clone>(
    links: &HashMap<String, T>,
) -> Option<HashMap<String, T>> {
    let ret = links
        .iter()
        .filter(|(k, _)| is_platform_package(k))
        .map(|(k, v)| (generator::ensure_heroku_sys_prefix(k), v.clone()))
        .collect::<HashMap<_, _>>();

    ret.is_empty().not().then_some(ret)
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PlatformFinalizerError {
    RuntimeRequirementInRequireDevButNotRequire,
}

#[derive(Debug, Eq, Hash, PartialEq)]
pub(crate) enum PlatformFinalizerNotice {
    RuntimeRequirementInserted(String, String),
    RuntimeRequirementFromDependencies,
}

// From the given [`ComposerPackage`], creates a new one with the same name and version, and any package links processed through [`extract_platform_links_with_heroku_sys`].
//
// Package links can reside in four different places on a Composer package:
// - `require`
// - `provide`
// - `conflict`
// - `replace`
pub(crate) fn package_with_only_platform_links(
    package: &ComposerPackage,
) -> Option<ComposerPackage> {
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
            kind: Some("metapackage".to_string()),
            require,
            provide,
            conflict,
            replace,
            ..Default::default()
        },
    })
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ComposerLockVersionNotice {
    NoComposerPluginApiVersionInLock(String),
    ComposerPluginApiVersionConfined(String, String),
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ComposerLockVersionError {
    InvalidPlatformApiVersion(String),
}

/// Generates requirements for Composer and the Composer Plugin API version that match the given [`ComposerLock`].
///
/// The returned [`Warned`] struct contains a hash map of the generated requirements, and a list of [`PlatformExtractorNotice`s](PlatformExtractorNotice) encountered during processing.
fn requires_for_composer_itself(
    lock: &ComposerLock,
) -> Result<Warned<HashMap<String, String>, ComposerLockVersionNotice>, ComposerLockVersionError> {
    let mut notices = Vec::new();
    let mut requires = HashMap::new();
    // we want the latest Composer...
    requires.insert(
        generator::ensure_heroku_sys_prefix("composer"),
        "*".to_string(),
    );
    // ... that supports the major plugin API version from the lock file (which corresponds to the Composer version series, so e.g. all 2.3.x releases have 2.3.0)
    // if the lock file says "2.99.0", we generally still want to select "^2", and not "^2.99.0"
    // this is so the currently available Composer version can install lock files generated by brand new or pre-release Composer versions, as this stuff is generally forward compatible
    // otherwise, builds would fail the moment e.g. 2.6.0 comes out and people try it, even though 2.5 could install the project just fine
    requires.insert(
        generator::ensure_heroku_sys_prefix("composer-plugin-api"),
        match lock.plugin_api_version.as_deref() {
            // no rule without an exception, of course:
            // there are quite a lot of BC breaks for plugins in Composer 2.3
            // if the lock file was generated with 2.0, 2.1 or 2.2, we play it safe and install 2.2.x (which is LTS)
            // this is mostly to ensure any plugins that have an open enough version selector do not break with all the 2.3 changes
            // also ensures plugins are compatible with other libraries Composer bundles (e.g. various Symfony components), as those got big version bumps in 2.3
            Some(v @ ("2.0.0" | "2.1.0" | "2.2.0")) => {
                let r = "~2.2.0".to_string();
                notices.push(ComposerLockVersionNotice::ComposerPluginApiVersionConfined(
                    v.to_string(),
                    r.clone(),
                ));
                r
            }
            // just "^2" or similar so we get the latest we have, see comment earlier
            Some(v) => format!(
                "^{}",
                v.split_once('.')
                    .ok_or(ComposerLockVersionError::InvalidPlatformApiVersion(
                        v.to_string()
                    ))?
                    .0
            ),
            // nothing means it's pre-v1.10, in which case we want to just use v1
            None => {
                let r = "^1.0.0".to_string();
                notices.push(ComposerLockVersionNotice::NoComposerPluginApiVersionInLock(
                    r.clone(),
                ));
                r
            }
        },
    );
    Ok(Warned::new(requires, notices))
}

/// From the given [`ComposerLock`], extracts all relevant fields into a [`PlatformJsonGeneratorInput`].
///
/// The returned [`Warned`] struct contains the generated input struct, and a list of [`PlatformExtractorNotice`s](PlatformExtractorNotice) encountered during processing.
pub(crate) fn extract_from_lock(
    lock: &ComposerLock,
) -> Result<Warned<PlatformJsonGeneratorInput, PlatformExtractorNotice>, PlatformExtractorError> {
    let mut config = PlatformJsonGeneratorInput::from(lock);
    let composer_requires =
        requires_for_composer_itself(lock).map_err(PlatformExtractorError::ComposerLockVersion)?;

    let mut processing_notices = Vec::new();
    config
        .additional_require
        .replace(composer_requires.unwrap(&mut processing_notices)); // Warned::unwrap does not panic :)

    Ok(Warned::new(
        config,
        processing_notices
            .into_iter()
            .map(PlatformExtractorNotice::ComposerLockVersion),
    ))
}

/// Post-processes the given [`ComposerRootPackage`] to insert a runtime requirement, if necessary (and possible).
///
/// The returned value is a list of [`PlatformFinalizerNotice`s](PlatformFinalizerNotice) to indicate what operations were performed.
pub(crate) fn ensure_runtime_requirement(
    root_package: &mut ComposerRootPackage,
) -> Result<Vec<PlatformFinalizerNotice>, PlatformFinalizerError> {
    let mut notices = Vec::new();

    let repositories = root_package
        .package
        .repositories
        .clone()
        .unwrap_or_default();
    // from all our metapackages, dev or not, make a lookup table
    let metapackages = repositories
        .iter()
        .filter(|repo| matches!(repo, ComposerRepository::Package { .. }))
        .fold(HashMap::new(), |mut acc, repo| match repo {
            ComposerRepository::Package { package, .. } => {
                acc.extend(
                    package
                        .iter()
                        .map(|package| (package.name.clone(), package)),
                );
                acc
            }
            _ => acc,
        });

    // is there a requirement for php in the root?
    if !has_runtime_link(&root_package.package.require.clone().unwrap_or_default()) {
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
                *marker |= metapackages.get(&name).is_some_and(|package| {
                    // here, we only look at a package's require list, not require-dev, which only has an effect in the root of a composer.json
                    // (since every library has its own list of dev requirements for testing etc, and that should never be installed into a project using that library)
                    has_runtime_link(&package.package.require.clone().unwrap_or_default())
                });
            }
        }

        if seen_runtime_requirement {
            // some dependenc(y|ies) required a runtime, which will be used
            notices.push(PlatformFinalizerNotice::RuntimeRequirementFromDependencies);
        } else if seen_runtime_dev_requirement {
            // no runtime requirement anywhere, but there is a requirement in a require-dev package, which we do not allow
            return Err(PlatformFinalizerError::RuntimeRequirementInRequireDevButNotRequire);
        } else {
            // nothing anywhere; time to insert a default!
            let name = "php".to_string();
            let version = "*".to_string();
            notices.push(PlatformFinalizerNotice::RuntimeRequirementInserted(
                name.clone(),
                version.clone(),
            ));
            // TODO: if we have seen `php-…` variants like `php-ipv6` or `php-zts`, should we warn or fail?
            root_package
                .package
                .require
                .get_or_insert(HashMap::new()) // could be None
                .insert(generator::ensure_heroku_sys_prefix(name), version);
        }
    }

    // TODO: generate conflict entries for php-zts, php-debug?

    Ok(notices)
}
