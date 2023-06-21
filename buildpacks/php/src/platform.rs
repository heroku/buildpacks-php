pub(crate) mod generator;

use crate::PhpBuildpack;
use composer::ComposerRootPackage;
use libcnb::build::BuildContext;
use libcnb::Platform;
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
