pub(crate) mod generator;

use crate::PhpBuildpack;
use crate::bootstrap;
use crate::platform::generator::PlatformGeneratorError;
use composer::ComposerRootPackage;
use indexmap::IndexMap;
use libcnb::build::BuildContext;
use libcnb::{Platform, Target};
use std::str::FromStr;
use url::Url;

enum UrlListEntry {
    Default,
    Reset,
    Url(Url),
}

impl FromStr for UrlListEntry {
    type Err = url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "$default"|"${default}" => Ok(Self::Default),
            "-" => Ok(Self::Reset),
            v => Url::parse(v).map(Self::Url),
        }
    }
}

#[derive(Debug)]
pub(crate) enum PlatformRepositoryUrlError {
    Split(shell_words::ParseError),
    Parse(url::ParseError),
    Empty,
}

pub(crate) fn heroku_stack_name_for_target(target: &Target) -> Result<String, String> {
    let Target {
        os,
        distro_name,
        distro_version,
        ..
    } = target;
    match (os.as_str(), distro_name.as_str(), distro_version.as_str()) {
        ("linux", "ubuntu", v @ ("22.04" | "24.04")) => {
            Ok(format!("heroku-{}", v.strip_suffix(".04").unwrap_or(v)))
        }
        _ => Err(format!("{os}-{distro_name}-{distro_version}")),
    }
}

pub(crate) fn default_platform_repository_url_for_target(target: &Target) -> Url {
    let Target {
        os,
        arch,
        distro_name,
        distro_version,
        ..
    } = target;
    let stack_identifier = if let ("linux", "ubuntu", _) =
        (os.as_str(), distro_name.as_str(), distro_version.as_str())
    {
        let stack_name = heroku_stack_name_for_target(target)
            .expect("Internal error: could not determine Heroku stack name for OS/distro");
        format!("{stack_name}-{arch}")
    } else {
        format!("{os}-{arch}-{distro_name}-{distro_version}")
    };

    Url::parse(&format!(
        "https://heroku-buildpack-php.s3.dualstack.us-east-1.amazonaws.com/dist-{stack_identifier}-stable/",
    ))
    .expect("Internal error: failed to generate default repository URL")
    .join(format!("packages-{}.json", bootstrap::PLATFORM_REPOSITORY_SNAPSHOT).as_str())
    .expect("Internal error: failed to generate default repository URL")
}

/// Returns a list of platform repository [`Url`s](Url), computed from the given [`BuildContext`]'s
/// stack ID and processed `HEROKU_PHP_PLATFORM_REPOSITORIES` environment variable.
///
/// Defers to [`platform_repository_urls_from_defaults_and_list`] once a default URL string has been constructed and
/// the `HEROKU_PHP_PLATFORM_REPOSITORIES` environment variable has been read.
pub(crate) fn platform_repository_urls_from_default_and_build_context(
    context: &BuildContext<PhpBuildpack>,
) -> Result<(Url, Vec<Url>), PlatformRepositoryUrlError> {

    // anything user-supplied
    let user_repos = context
        .platform
        .env()
        .get_string_lossy("HEROKU_PHP_PLATFORM_REPOSITORIES")
        .unwrap_or_default();

    platform_repository_urls_from_default_and_list(
        default_platform_repository_url_for_target(&context.target),
        user_repos
    )
    // TODO: message if default disabled?
    // TODO: message for additional repos?
}

/// Returns a list of platform repository [`Url`s](Url), computed from the given default [`Url`s](Url)
/// and space-separated list of additional URL strings (typically user-supplied).
fn platform_repository_urls_from_default_and_list(
    default_url: Url,
    extra_urls_list: impl AsRef<str>,
) -> Result<(Url, Vec<Url>), PlatformRepositoryUrlError> {
    let extra_urls_splits =
        shell_words::split(extra_urls_list.as_ref()).map_err(PlatformRepositoryUrlError::Split)?;
    let urls: Vec<Option<Url>> = ["$default".to_string()]
        .into_iter()
        .chain(extra_urls_splits.into_iter())
        .map(|v| { dbg!("value in url list:", &v); v.parse::<UrlListEntry>() })
        .collect::<Result<Vec<_>, _>>()
        .map_err(PlatformRepositoryUrlError::Parse)?
        .into_iter().map(|ule| {
            match ule {
                UrlListEntry::Default => Some(default_url.clone()),
                UrlListEntry::Url(url) => Some(url),
                UrlListEntry::Reset => None,
            }
        })
        .collect();
    dbg!("urls:", &urls);
    let mut splits = urls
        .split(Option::is_none);
    // We now have one or more slices, split by UrlListEntry::Reset ("-" in the string).
    // Only the last slice will be used as the list of repository URLs.
    // However, it's possible to re-set the list more than once;
    // this is useful with "full" custom repositories, to still bootstrap from $default.
    // Examples:
    // - Input: "https://my.repo/" (only a single custom repo added to built-in default)
    //   - becomes "$default https://my.repo/"
    //   - effective userland repos used: ["$default", "https://my.repo/"]
    //   - boostrap from: "$default"
    // - Input: "- https://my.repo/ https://myother.repo/" (fully replace built-in default with two custom repos)
    //   - becomes "$default - https://my.repo/ https://myother.repo/"
    //   - effective userland repos used: ["https://my.repo/", "https://myother.repo/"]
    //   - boostrap from: "https://my.repo/"
    // - Input: "- $default - https://my.repo/" (fully replace built-in default with custom repo, but bootstrap from default)
    //   - becomes "$default - $default - https://my.repo/"
    //   - effective userland repos used: ["https://my.repo/"]
    //   - boostrap from: "$default"
    // - Input: "- https://my.repo/ - $default" (bootstrap from custom repo without replacing default repo)
    //   - becomes "$default - https://my.repo/ - $default"
    //   - effective userland repos used: ["$default"]
    //   - boostrap from: "https://my.repo/"

    let mut bootstrap: Option<Url> = None;
    // We start with the first item in the slice.
    dbg!("splits:", &splits);
    let mut repositories = splits.next()// split() always returns at least one slice
        .unwrap_or_else(|| unreachable!("Something is rotten in the state of Denmark."));
    // If we have remaining splits, we want to use those instead
    for split in splits {
        dbg!("iteration in splits:", split);
        // Only for the first split we encounter, we want to use the first URL for bootstrapping
        if bootstrap.is_none() {
            // FIXME: unwrap
            bootstrap = split.first().unwrap().clone();
            dbg!("bootstrap:", &bootstrap);
        }
        // Only the last split wins as the final list of repository URLs
        repositories = split;
    };

    // FIXME: unwrap
    Ok((bootstrap.unwrap_or(repositories.first().unwrap().clone().unwrap()), repositories.into_iter().cloned().collect::<Option<Vec<_>>>().ok_or(PlatformRepositoryUrlError::Empty)?))
}

/// For a given [`UrlListEntry`] slice, returns a [`Vec<&Url>`] containing only the inner [`Url`]
/// values of all [`UrlListEntry::Url`] variants that follow the last [`UrlListEntry::Reset`] in the
/// slice (or of all [`UrlListEntry::Url`] variants if no [`UrlListEntry::Reset`] is present).
fn normalize_url_list<'a>(urls: &'a [UrlListEntry], default_url: &'a Url) -> impl Iterator<Item = &'a Url> {
    // we now have a list of URLs
    // some of these entries might be UrlListEntry::Reset, used to re-set anything to their left (i.e. typically the default repo)
    // we want all entries after the last UrlListEntry::Reset
    urls.rsplit(|url_entry| matches!(url_entry, UrlListEntry::Reset))
        .next() // this iterator is never empty because rsplit() will always return at least an empty slice
        .unwrap_or_else(|| unreachable!("Something is rotten in the state of Denmark."))
        .iter()
        .map(move |url_entry| match url_entry {
            UrlListEntry::Default => default_url,
            UrlListEntry::Url(url) => &url,
            UrlListEntry::Reset => unreachable!(
                "If you can see this message, you broke the rsplit predicate a few lines up."
            ),
        })
}

#[derive(Debug)]
pub(crate) enum WebserversJsonError {
    PlatformGenerator(PlatformGeneratorError),
}

pub(crate) fn webservers_json(
    stack: &str,
    platform_repositories: &[Url],
) -> Result<ComposerRootPackage, WebserversJsonError> {
    let webservers_generator_input = generator::PlatformJsonGeneratorInput {
        additional_require: Some(IndexMap::from([
            ("heroku-sys/apache".to_string(), "*".to_string()),
            ("heroku-sys/nginx".to_string(), "*".to_string()),
            // this package contains heroku-php-apache2 and heroku-php-nginx, plus runtime configs
            ("heroku-sys/boot-scripts".to_string(), "^1.0.0".to_string()),
        ])),
        ..Default::default()
    };

    let mut webservers_json = generator::generate_platform_json(
        &webservers_generator_input,
        stack,
        platform_repositories,
    )
    .map_err(WebserversJsonError::PlatformGenerator)?;

    //
    let mut config_with_bin_dir = webservers_json.config.unwrap_or_default();
    config_with_bin_dir.insert("bin-dir".to_string(), "bin".into());
    webservers_json.config = Some(config_with_bin_dir);

    Ok(webservers_json)
}
