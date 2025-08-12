// TODO: Switch to libcnb's struct layer API.
#![allow(deprecated)]

use crate::{PhpBuildpack, PhpBuildpackError};
use libcnb::build::BuildContext;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::generic::GenericMetadata;
use libcnb::layer::{Layer, LayerResult, LayerResultBuilder};
use libcnb::Buildpack;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;

pub(crate) struct UsageHelpLayer<'a> {
    pub(crate) help_texts: &'a HashMap<&'a str, &'a str>,
}

impl Layer for UsageHelpLayer<'_> {
    type Buildpack = PhpBuildpack;
    type Metadata = GenericMetadata;

    fn types(&self) -> LayerTypes {
        LayerTypes {
            build: false,
            cache: false,
            launch: true,
        }
    }

    fn create(
        &mut self,
        _context: &BuildContext<Self::Buildpack>,
        layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        for (process_type, help_text) in self.help_texts {
            let mut file = fs::File::create(layer_path.join(process_type).with_extension("txt"))
                .map_err(UsageHelpLayerError::FileWrite)?;
            // we join_art() two headers, because the gap between "Help:" and the process type name is too large otherwise
            let usage = text_to_ascii_art::to_art("Usage".into(), "default", 0, 0, 0)
                .map_err(UsageHelpLayerError::AsciiHeader)?;
            // the colon is also pretty wide, so we trim it to seven characters total, with more space on the right side
            let colon = text_to_ascii_art::to_art(":".into(), "default", 0, 0, 0)
                .map_err(UsageHelpLayerError::AsciiHeader)?
                .lines()
                .map(|line| format!(" {:6}", line.trim_ascii()))
                .collect::<Vec<String>>()
                .join("\n");
            let header = text_to_ascii_art::join_art(
                &text_to_ascii_art::join_art(&usage, &colon, 0),
                &text_to_ascii_art::to_art((*process_type).into(), "default", 0, 0, 0)
                    .map_err(UsageHelpLayerError::AsciiHeader)?,
                0,
            );
            file.write_fmt(format_args!("{header}\n\n{help_text}\n"))
                .map_err(UsageHelpLayerError::FileWrite)?;
        }
        LayerResultBuilder::new(GenericMetadata::default()).build()
    }
}

#[derive(Debug)]
pub(crate) enum UsageHelpLayerError {
    FileWrite(std::io::Error),
    AsciiHeader(String),
}

impl From<UsageHelpLayerError> for PhpBuildpackError {
    fn from(error: UsageHelpLayerError) -> Self {
        Self::UsageHelpLayer(error)
    }
}
