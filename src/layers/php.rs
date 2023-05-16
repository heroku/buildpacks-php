use crate::utils::{self};
use crate::{PhpBuildpack, PhpBuildpackError};

use libcnb::build::BuildContext;
use libcnb::data::buildpack::StackId;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::layer::{Layer, LayerResult, LayerResultBuilder};
use libcnb::layer_env::{LayerEnv, ModificationBehavior, Scope};

use libcnb::{Buildpack, Env};
use libherokubuildpack::log::{log_header, log_info};
use serde::de::{Error, Unexpected};
use serde::{Deserialize, Deserializer, Serialize};
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) struct PhpLayer {
    pub bootstrap_env: Env,
    pub composer_cache_layer_path: PathBuf,
    pub platform_json: String,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub(crate) struct PhpLayerMetadata {
    stack: StackId,
}

#[derive(Deserialize, Debug)]
struct LayerEnvValue {
    #[serde(deserialize_with = "scope_from_string")]
    scope: Scope,
    #[serde(deserialize_with = "modification_behavior_from_string")]
    modification_behavior: ModificationBehavior,
    name: String,
    value: String,
}

fn scope_from_string<'de, D>(deserializer: D) -> Result<Scope, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    Ok(match s.as_ref() {
        "build" => Scope::Build,
        "launch" => Scope::Launch,
        "all" => Scope::All,
        process => Scope::Process(process.to_string()),
    })
}

fn modification_behavior_from_string<'de, D>(
    deserializer: D,
) -> Result<ModificationBehavior, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = Deserialize::deserialize(deserializer)?;
    match s.as_ref() {
        "append" => Ok(ModificationBehavior::Append),
        "default" => Ok(ModificationBehavior::Default),
        "delimiter" => Ok(ModificationBehavior::Delimiter),
        "override" => Ok(ModificationBehavior::Override),
        "prepend" => Ok(ModificationBehavior::Prepend),
        _ => Err(D::Error::invalid_value(
            Unexpected::Str(&s),
            &"one of: append, default, delimiter, override, prepend",
        )),
    }
}

impl Layer for PhpLayer {
    type Buildpack = PhpBuildpack;
    type Metadata = PhpLayerMetadata;

    fn types(&self) -> LayerTypes {
        LayerTypes {
            build: true,
            cache: false,
            launch: true,
        }
    }

    fn create(
        &self,
        context: &BuildContext<Self::Buildpack>,
        layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        log_header("Installing platform packages");

        let mut platform_json = File::create(layer_path.join("composer.json"))
            .map_err(PhpLayerError::PlatformJsonCreate)?;
        platform_json
            .write_all(self.platform_json.as_ref())
            .map_err(PhpLayerError::PlatformJsonWrite)?;

        // the computed env vars for this layer are written to this JSON file by the installer
        let layer_env_file_path = layer_path.join("layer_env.json"); // TODO: truncate?
                                                                     // a log of native extensions not installed because of userland provides is written to this file
        let provided_extensions_log_file_path = layer_path.join("provided_extensions.tsv"); // TODO: truncate?

        utils::run_command(
            Command::new("composer")
                .current_dir(layer_path)
                .envs(&self.bootstrap_env) // we're invoking 'composer' from the bootstrap layer
                .env("COMPOSER_HOME", &self.composer_cache_layer_path)
                .args([
                    "install",
                    "--no-dev",
                    "--no-interaction",
                    //"--no-progress",
                ])
                .env("layer_env_file_path", &layer_env_file_path)
                .env(
                    "providedextensionslog_file_path",
                    &provided_extensions_log_file_path,
                ),
        )
        .expect("platform install failed"); // FIXME: handle

        // FIXME: we have to do that now, not later, since the installer gets invoked again
        // ^ to be solved on the installer side, which has to merge the values from later calls...

        // our platform installer plugin writes out a JSON file with env vars that we have to set
        // this is because many packages add to the path, set env var defaults, etc, and we cannot hard-code those in here
        // the values are pre-assembled in case of prepend/append, since only a single value may be set per env var in these cases
        let layer_env_values: Vec<LayerEnvValue> = serde_json::from_reader(BufReader::new(
            File::open(&layer_env_file_path).expect("Failed to open layer_env_file_path"),
        ))
        .expect("Failed to parse layer_env_file_path JSON");

        // populate our layer with these variables
        let mut layer_env = LayerEnv::new();
        for data in layer_env_values {
            layer_env.insert(
                data.scope,
                data.modification_behavior,
                data.name,
                data.value,
            );
        }

        // not all native extensions might have gotten installed due to userland provide declarations
        // we now go over the "log" generated by the installer plugin and read the provides
        // then we attempt to force-install the provided packages as native variants
        // format is lines of fields separated by spaces (FIXME: tabs):
        // - first is name of "provider" package
        // - remaining fields are the native packages it claimed as provided
        // the great thing is that we process these install attempts in the order the solver originally produced them
        // which means that we install more "important" extensions, that later ones might depend on, first
        // that, in turn, is important for loading order into PHP
        if let Ok(mut rdr) = csv::ReaderBuilder::new()
            .delimiter(b' ') // FIXME: switch to tabs here and in installer plugin
            .has_headers(false)
            .flexible(true) // variable number of "fields"
            .from_path(&provided_extensions_log_file_path)
        {
            let mut composer_require_base = Command::new("composer");
            // FIXME: lol why does it need this let binding...
            let composer_require_base = composer_require_base
                .current_dir(layer_path)
                .envs(&self.bootstrap_env) // we're invoking 'composer' from the bootstrap layer
                // .env("layer_env_file_path", &layer_env_file_path)
                .env("COMPOSER_HOME", &self.composer_cache_layer_path);
            for result in rdr.deserialize() {
                let (provider, provides): (String, Vec<String>) = result.unwrap(); // FIXME: handle
                log_info(format!(
                    "Attempting native extension installs for {}",
                    provider
                ));

                for provide in provides {
                    let (name, _version) = provide.split_once(":").unwrap();
                    utils::run_command(composer_require_base.args([
                        "require",
                        &format!("{}.native:*", name),
                        // "--no-dev",
                        // "--no-interaction",
                        //"--no-progress",
                    ]))
                    .expect("install attempt failed"); // FIXME: handle gracefully
                }
            }
        }

        let layer_metadata = generate_layer_metadata(&context.stack_id);
        LayerResultBuilder::new(layer_metadata)
            .env(layer_env)
            .build()
    }
}

fn generate_layer_metadata(stack_id: &StackId) -> PhpLayerMetadata {
    PhpLayerMetadata {
        stack: stack_id.clone(),
    }
}

#[derive(Debug)]
pub(crate) enum PhpLayerError {
    PlatformJsonCreate(std::io::Error),
    PlatformJsonWrite(std::io::Error),
}

impl From<PhpLayerError> for PhpBuildpackError {
    fn from(error: PhpLayerError) -> Self {
        Self::PhpLayer(error)
    }
}
