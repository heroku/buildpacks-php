use flate2::read::GzDecoder;
use std::io;
use std::path::Path;
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

pub(crate) fn download_and_unpack_gzip(
    uri: &str,
    destination: &Path,
) -> Result<(), DownloadUnpackError> {
    // TODO: Timeouts: https://docs.rs/ureq/latest/ureq/struct.AgentBuilder.html?search=timeout
    let response = ureq::get(uri)
        .call()
        .map_err(|err| DownloadUnpackError::Request(Box::new(err)))?;
    let gzip_decoder = GzDecoder::new(response.into_reader());
    Archive::new(gzip_decoder)
        .unpack(destination)
        .map_err(DownloadUnpackError::Io)
}

#[derive(Debug)]
pub(crate) enum DownloadUnpackError {
    Io(io::Error),
    // Boxed to prevent `large_enum_variant` Clippy errors since `ureq::Error` is massive.
    Request(Box<ureq::Error>), // TODO: does this still need boxing?
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

#[derive(Debug)]
pub(crate) enum CommandError {
    Io(io::Error),
    NonZeroExitStatus(ExitStatus),
}
