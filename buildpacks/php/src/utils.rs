use flate2::read::GzDecoder;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitStatus};
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
    Request(Box<ureq::Error>), // TODO: does this still need boxing?
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
    Archive::new(GzDecoder::new(download(uri)?))
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
    // TODO: Timeouts: https://docs.rs/ureq/latest/ureq/struct.AgentBuilder.html?search=timeout
    Ok(ureq::get(uri)
        .call()
        .map_err(|err| DownloadUnpackError::Request(Box::new(err)))?
        .into_reader())
}

#[derive(Debug)]
pub(crate) enum CommandError {
    Io(io::Error),
    NonZeroExitStatus(ExitStatus),
}

pub(crate) fn run_command(command: &mut Command) -> Result<(), CommandError> {
    command
        .status()
        .map_err(CommandError::Io)
        .and_then(|exit_status| {
            if exit_status.success() {
                Ok(())
            } else {
                Err(CommandError::NonZeroExitStatus(exit_status))
            }
        })
}
