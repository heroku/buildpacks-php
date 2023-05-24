use crate::PhpBuildpack;
use libcnb::build::BuildContext;
use libcnb::Platform;
use std::str::FromStr;
use url::Url;

pub(crate) mod generator;

#[derive(Debug)]
pub(crate) enum RepoUrlsError {
    SplitError(shell_words::ParseError),
    ParseError(url::ParseError),
}

impl From<shell_words::ParseError> for RepoUrlsError {
    fn from(err: shell_words::ParseError) -> Self {
        Self::SplitError(err)
    }
}

impl From<url::ParseError> for RepoUrlsError {
    fn from(err: url::ParseError) -> Self {
        Self::ParseError(err)
    }
}

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

pub(crate) fn repos_from_default_and_env(
    context: &BuildContext<PhpBuildpack>,
) -> Result<Vec<Url>, RepoUrlsError> {
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
        .unwrap_or("".into());

    repos_from_defaults_and_list(&default_platform_repositories, &user_repos)
    // TODO: message if default disabled?
    // TODO: message for additional repos?
}

pub(crate) fn repos_from_defaults_and_list(
    default_urls: &[Url],
    extra_urls_list: impl AsRef<str>,
) -> Result<Vec<Url>, RepoUrlsError> {
    let extra_urls_splits = shell_words::split(extra_urls_list.as_ref())?;
    default_urls
        .into_iter()
        .cloned()
        .map(UrlListEntry::Url)
        .map(Ok)
        .chain(extra_urls_splits.into_iter().map(|v| v.parse()))
        .collect::<Result<Vec<_>, _>>()
        .map(|repos| normalize_url_list(&repos).into_iter().cloned().collect())
        .map_err(RepoUrlsError::ParseError)
}

fn normalize_url_list(urls: &[UrlListEntry]) -> Vec<&Url> {
    // we now have a list of URLs
    // some of these entries might be UrlListEntry::Reset, used to re-set anything to their left (i.e. typically the default repo)
    // we want all entries after the last UrlListEntry::Reset
    urls.rsplit(|url_entry| matches!(url_entry, UrlListEntry::Reset))
        .next() // this iterator is never empty because rsplit() will always return at least an empty slice
        .unwrap_or_else(|| unreachable!("Something is rotten in the state of Denmark."))
        .into_iter()
        .map(|url_entry| match url_entry {
            UrlListEntry::Url(url) => url,
            UrlListEntry::Reset => unreachable!(
                "If you can see this message, you broke the rsplit predicate a few lines up."
            ),
        })
        .collect()
}
