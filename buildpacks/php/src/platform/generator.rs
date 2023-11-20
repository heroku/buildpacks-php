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
    list.split(sep).map(str::trim).filter(|&p| !p.is_empty())
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
    pub(crate) minimum_stability: ComposerStability,
    /// The desired value for the root package's `prefer-stable` field
    pub(crate) prefer_stable: bool,
    /// The direct platform requirements from the root dependencies of the source project
    pub(crate) platform_require: HashMap<String, String>,
    /// The direct platform dev requirements from the root dependencies of the source project
    pub(crate) platform_require_dev: HashMap<String, String>,
    /// A list of packages from the source project's locked dependencies
    pub(crate) packages: Vec<ComposerPackage>,
    /// A list of packages from the source project's locked dev dependencies
    pub(crate) packages_dev: Vec<ComposerPackage>,
    /// A list of additional requirements to be placed into the generated package's root requirements
    pub(crate) additional_require: Option<HashMap<String, String>>,
    /// A list of additional requirements to be placed into the generated package's root dev requirements
    pub(crate) additional_require_dev: Option<HashMap<String, String>>,
    /// Additional [`ComposerRepository`] entries to be placed into the generated package
    pub(crate) additional_repositories: Option<Vec<ComposerRepository>>,
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
