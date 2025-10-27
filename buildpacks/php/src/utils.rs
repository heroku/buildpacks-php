use flate2::read::GzDecoder;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use tar::Archive;

macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        RE.get_or_init(|| {
            regex::Regex::new($re).expect("Internal error: failed to compile regular expression.")
        })
    }};
}
pub(crate) use regex;

#[derive(Debug)]
pub(crate) enum DownloadUnpackError {
    Io(io::Error),
    // Boxed to prevent `large_enum_variant` Clippy errors since `ureq::Error` is massive.
    Request(Box<ureq::Error>),
}

#[allow(unused)]
pub(crate) fn download_and_unpack_tgz(
    uri: &str,
    destination: &Path,
) -> Result<(), DownloadUnpackError> {
    download_and_unpack_tgz_with_components_stripped(uri, destination, 0)
}

#[allow(unused)]
pub(crate) fn download_and_unpack_tgz_with_components_stripped(
    uri: &str,
    destination: &Path,
    strip_components: usize,
) -> Result<(), DownloadUnpackError> {
    download_and_unpack_tgz_with_components_stripped_and_only_entries_under_prefix(
        uri,
        destination,
        strip_components,
        PathBuf::new(),
    )
}

pub(crate) fn download_and_unpack_tgz_with_components_stripped_and_only_entries_under_prefix(
    uri: &str,
    destination: &Path,
    strip_components: usize,
    extract_only_prefix: impl AsRef<Path>,
) -> Result<(), DownloadUnpackError> {
    Archive::new(GzDecoder::new(download_with_retry(uri)?))
        .entries()
        .map_err(DownloadUnpackError::Io)? // discard invalid entries
        .filter_map(Result::ok)
        .try_for_each(|mut entry| {
            entry
                .path()
                .map_err(DownloadUnpackError::Io)? // in case of invalid paths
                .components()
                // consume any leading non-"normal" path components, e.g. a possible "."
                .skip_while(|component| !matches!(component, Component::Normal(_)))
                // strip number of given component entries
                .skip(strip_components)
                .collect::<PathBuf>()
                .strip_prefix(&extract_only_prefix)
                // if strip_prefix worked, it means we want to extract that entry (with the prefix stripped)
                // otherwise, we simply ignore the entry, since we do not want to extract it
                .map_or(Ok(()), |path| {
                    entry
                        .unpack(destination.join(path))
                        .map_err(DownloadUnpackError::Io)
                        .map(|_| ()) // instead of returning the Unpacked struct from unpack()
                })
        })
}

fn download(uri: &str) -> Result<Box<dyn io::Read + Send + Sync + 'static>, DownloadUnpackError> {
    // TODO: Timeouts once we move to ureq v3 (as it has ConfigBuilder::timeout_resolve())
    Ok(ureq::get(uri)
        .call()
        .map_err(|err| DownloadUnpackError::Request(Box::new(err)))?
        .into_reader())
}

fn download_with_retry(
    uri: &str,
) -> Result<Box<dyn io::Read + Send + Sync + 'static>, DownloadUnpackError> {
    let backoff =
        exponential_backoff::Backoff::new(3, Duration::from_secs(1), Duration::from_secs(10));

    let mut backoff_durations = backoff.into_iter();
    loop {
        match download(uri) {
            result @ Ok(_) => return result,
            result @ Err(_) => match backoff_durations.next() {
                None | Some(None) => return result,
                Some(Some(backoff_duration)) => {
                    std::thread::sleep(backoff_duration);
                }
            },
        }
    }
}

pub(crate) fn add_prefix_to_non_empty<P: Into<Vec<u8>>>(prefix: P) -> impl Fn(Vec<u8>) -> Vec<u8> {
    let prefix = prefix.into();

    move |mut input| {
        if input.is_empty() {
            vec![]
        } else {
            let mut result = prefix.clone();
            result.append(&mut input);
            result
        }
    }
}
