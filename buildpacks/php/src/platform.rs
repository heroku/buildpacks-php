pub(crate) mod generator;

use crate::platform::generator::PlatformGeneratorError;
use crate::PhpBuildpack;
use composer::ComposerRootPackage;
use libcnb::build::BuildContext;
use libcnb::Platform;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use url::Url;

enum UrlListEntry {
    Reset,
    Url(Url),
}

impl FromStr for UrlListEntry {
    type Err = url::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "-" => Ok(Self::Reset),
            v => Url::parse(v).map(Self::Url),
        }
    }
}

#[derive(Debug)]
pub(crate) enum PlatformRepositoryUrlError {
    Split(shell_words::ParseError),
    Parse(url::ParseError),
}

/// Returns a list of platform repository [`Url`s](Url), computed from the given [`BuildContext`]'s
/// stack ID and processed `HEROKU_PHP_PLATFORM_REPOSITORIES` environment variable.
///
/// Defers to [`platform_repository_urls_from_defaults_and_list`] once a default URL string has been constructed and
/// the `HEROKU_PHP_PLATFORM_REPOSITORIES` environment variable has been read.
pub(crate) fn platform_repository_urls_from_default_and_build_context(
    context: &BuildContext<PhpBuildpack>,
) -> Result<Vec<Url>, PlatformRepositoryUrlError> {
    // our default repo
    let default_platform_repositories = vec![Url::parse(&format!(
        "https://lang-php.s3.us-east-1.amazonaws.com/dist-{}-cnb/",
        context.stack_id,
    ))
    .expect("Internal error: failed to parse default repository URL")];

    // anything user-supplied
    let user_repos = context
        .platform
        .env()
        .get_string_lossy("HEROKU_PHP_PLATFORM_REPOSITORIES")
        .unwrap_or(String::new());

    platform_repository_urls_from_defaults_and_list(&default_platform_repositories, user_repos)
    // TODO: message if default disabled?
    // TODO: message for additional repos?
}

/// Returns a list of platform repository [`Url`s](Url), computed from the given default [`Url`s](Url)
/// and space-separated list of additional URL strings (typically user-supplied).
pub(crate) fn platform_repository_urls_from_defaults_and_list(
    default_urls: &[Url],
    extra_urls_list: impl AsRef<str>,
) -> Result<Vec<Url>, PlatformRepositoryUrlError> {
    let extra_urls_splits =
        shell_words::split(extra_urls_list.as_ref()).map_err(PlatformRepositoryUrlError::Split)?;
    default_urls
        .iter()
        .cloned()
        .map(UrlListEntry::Url)
        .map(Ok)
        .chain(extra_urls_splits.into_iter().map(|v| v.parse()))
        .collect::<Result<Vec<_>, _>>()
        .map(|repos| normalize_url_list(&repos).into_iter().cloned().collect())
        .map_err(PlatformRepositoryUrlError::Parse)
}

/// For a given [`UrlListEntry`] slice, returns a [`Vec<&Url>`] containing only the inner [`Url`]
/// values of all [`UrlListEntry::Url`] variants that follow the last [`UrlListEntry::Reset`] in the
/// slice (or of all [`UrlListEntry::Url`] variants if no [`UrlListEntry::Reset`] is present).
fn normalize_url_list(urls: &[UrlListEntry]) -> Vec<&Url> {
    // we now have a list of URLs
    // some of these entries might be UrlListEntry::Reset, used to re-set anything to their left (i.e. typically the default repo)
    // we want all entries after the last UrlListEntry::Reset
    urls.rsplit(|url_entry| matches!(url_entry, UrlListEntry::Reset))
        .next() // this iterator is never empty because rsplit() will always return at least an empty slice
        .unwrap_or_else(|| unreachable!("Something is rotten in the state of Denmark."))
        .iter()
        .map(|url_entry| match url_entry {
            UrlListEntry::Url(url) => url,
            UrlListEntry::Reset => unreachable!(
                "If you can see this message, you broke the rsplit predicate a few lines up."
            ),
        })
        .collect()
}

#[derive(Debug)]
pub(crate) enum WebserversJsonError {
    PlatformGenerator(PlatformGeneratorError),
}

pub(crate) fn webservers_json(
    stack: &str,
    installer_path: &Path,
    classic_buildpack_path: &Path,
    platform_repositories: &Vec<Url>,
) -> Result<ComposerRootPackage, WebserversJsonError> {
    let webservers_generator_input = generator::PlatformJsonGeneratorInput {
        additional_require: Some(HashMap::from([
            ("heroku-sys/apache".to_string(), "*".to_string()),
            ("heroku-sys/nginx".to_string(), "*".to_string()),
            // for now, we need the web server boot scripts and configs from the classic buildpack
            (
                "heroku/heroku-buildpack-php".to_string(),
                "dev-bundled".to_string(),
            ),
        ])),
        // path repo for the above heroku/heroku-buildpack-php package
        additional_repositories: Some(vec![composer::ComposerRepository::from_path_with_options(
            classic_buildpack_path,
            [
                ("symlink".into(), serde_json::Value::Bool(false)),
                (
                    "versions".to_string(),
                    serde_json::json!({"heroku/heroku-buildpack-php": "dev-bundled"}),
                ),
            ],
        )]),
        ..Default::default()
    };

    let mut webservers_json = generator::generate_platform_json(
        &webservers_generator_input,
        stack,
        installer_path,
        platform_repositories,
    )
    .map_err(WebserversJsonError::PlatformGenerator)?;

    //
    let mut config_with_bin_dir = webservers_json.config.unwrap_or_default();
    config_with_bin_dir.insert("bin-dir".to_string(), "bin".into());
    webservers_json.config = Some(config_with_bin_dir);

    Ok(webservers_json)
}
