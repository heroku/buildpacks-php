use crate::layers::composer_cache::ComposerCacheLayer;
use crate::layers::composer_env::ComposerEnvLayer;
use crate::platform::generator;
use crate::platform::generator::PlatformJsonGeneratorInput;
use crate::utils::regex;
use crate::{utils, PhpBuildpack};
use composer::{
    ComposerBasePackage, ComposerLock, ComposerPackage, ComposerRepository, ComposerRootPackage,
};
use libcnb::build::BuildContext;
use libcnb::data::layer_name;
use libcnb::layer_env::Scope;
use libcnb::Env;
use libherokubuildpack::log::log_header;
use std::collections::{HashMap, HashSet};
use std::ops::Not;
use std::process::Command;

pub(crate) fn install_dependencies(
    context: &BuildContext<PhpBuildpack>,
    command_env: &mut Env,
) -> Result<(), String> {
    dbg!(&command_env);
    // TODO: check for presence of `vendor` dir
    // TODO: validate COMPOSER_AUTH?
    let composer_cache_layer = context
        .handle_layer(layer_name!("composer_cache"), ComposerCacheLayer)
        .unwrap(); // FIXME: handle
    dbg!(&composer_cache_layer.env);

    *command_env = composer_cache_layer.env.apply(Scope::Build, command_env);
    dbg!(&command_env);

    log_header("Installing dependencies");

    utils::run_command(
        Command::new("composer")
            .current_dir(&context.app_dir)
            .envs(&*command_env)
            .args([
                "install",
                "-vv",
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
    .expect("composer install failed"); // FIXME: handle

    // this just puts the userland bin-dir on $PATH
    let composer_env_layer = context
        .handle_layer(
            layer_name!("composer_env"),
            ComposerEnvLayer {
                command_env: command_env,
                dir: &context.app_dir,
            },
        )
        .unwrap(); // FIXME: handle
    dbg!(&composer_env_layer.env);
    *command_env = composer_env_layer.env.apply(Scope::All, command_env);
    dbg!(&command_env);

    // TODO: run `composer compile`, but is that still a good name?

    Ok(())
}

#[derive(strum_macros::Display, Debug, Eq, PartialEq)]
pub(crate) enum PlatformExtractorError {
    InvalidPlatformApiVersion,
}
#[derive(strum_macros::Display, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PlatformExtractorNotice {
    NoComposerPluginApiVersionInLock(String),
    ComposerPluginApiVersionConfined(String, String),
}

pub(crate) fn is_platform_package(name: impl AsRef<str>) -> bool {
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

pub(crate) fn has_runtime_requirement(requires: &HashMap<String, String>) -> bool {
    requires.contains_key("heroku-sys/php")
}

pub(crate) fn extract_platform_links_with_heroku_sys<T: Clone>(
    links: &HashMap<String, T>,
) -> Option<HashMap<String, T>> {
    let ret = links
        .iter()
        .filter(|(k, _)| is_platform_package(k))
        .map(|(k, v)| (generator::ensure_heroku_sys_prefix(k), v.clone()))
        .collect::<HashMap<_, _>>();

    ret.is_empty().not().then_some(ret)
}

#[derive(strum_macros::Display, Debug, Eq, PartialEq)]
pub(crate) enum PlatformFinalizerError {
    RuntimeRequirementInRequireDevButNotRequire,
}

#[derive(strum_macros::Display, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PlatformFinalizerNotice {
    RuntimeRequirementInserted(String, String),
    RuntimeRequirementFromDependencies,
}

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

pub(crate) fn process_composer_version(
    lock: &ComposerLock,
) -> Result<(HashMap<String, String>, HashSet<PlatformExtractorNotice>), PlatformExtractorError> {
    let mut notices = HashSet::new();
    let mut requires = HashMap::new();
    // we want the latest Composer...
    requires.insert(
        generator::ensure_heroku_sys_prefix("composer").to_string(),
        "*".to_string(),
    );
    // ... that supports the major plugin API version from the lock file (which corresponds to the Composer version series, so e.g. all 2.3.x releases have 2.3.0)
    // if the lock file says "2.99.0", we generally still want to select "^2", and not "^2.99.0"
    // this is so the currently available Composer version can install lock files generated by brand new or pre-release Composer versions, as this stuff is generally forward compatible
    // otherwise, builds would fail the moment e.g. 2.6.0 comes out and people try it, even though 2.5 could install the project just fine
    requires.insert(
        generator::ensure_heroku_sys_prefix("composer-plugin-api").to_string(),
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

pub(crate) fn extract_root_requirements(
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

pub(crate) fn extract_from_lock(
    lock: &ComposerLock,
) -> Result<(PlatformJsonGeneratorInput, HashSet<PlatformExtractorNotice>), PlatformExtractorError>
{
    let mut config = PlatformJsonGeneratorInput::from(lock);
    let composer_requires = process_composer_version(lock)?;

    config.additional_require.replace(composer_requires.0);

    Ok((config, composer_requires.1))
}

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
                .insert(generator::ensure_heroku_sys_prefix(name), version);
        }
    }

    // TODO: generate conflict entries for php-zts, php-debug?

    Ok(notices)
}
