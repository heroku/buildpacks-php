// TODO: Switch to libcnb's struct layer API.
#![allow(deprecated)]

use crate::{PhpBuildpack, PhpBuildpackError};
use bullet_stream::global::print;
use command_fds::{CommandFdExt, FdMapping, FdMappingCollision};
use composer::ComposerRootPackage;
use fs_err::File;
use libcnb::build::BuildContext;
use libcnb::data::layer_content_metadata::LayerTypes;
use libcnb::layer::{Layer, LayerResult, LayerResultBuilder};
use libcnb::layer_env::{LayerEnv, ModificationBehavior, Scope};
use libcnb::{Buildpack, Env, Target};
use serde::de::{Error, Unexpected};
use serde::{Deserialize, Deserializer, Serialize};
use std::io::{BufReader, Read, Seek};
use std::os::fd::AsFd;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PlatformLayerMetadata {
    arch: String,
    distro_name: String,
    distro_version: String,
}

pub(crate) struct PlatformLayer<'a> {
    pub(crate) command_env: &'a Env,
    pub(crate) platform_json: &'a ComposerRootPackage,
}

impl Layer for PlatformLayer<'_> {
    type Buildpack = PhpBuildpack;
    type Metadata = PlatformLayerMetadata;

    fn types(&self) -> LayerTypes {
        LayerTypes {
            build: true,
            cache: false,
            launch: true,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn create(
        &mut self,
        context: &BuildContext<Self::Buildpack>,
        layer_path: &Path,
    ) -> Result<LayerResult<Self::Metadata>, <Self::Buildpack as Buildpack>::Error> {
        let platform_json = File::create(layer_path.join("composer.json"))
            .map_err(PlatformLayerError::PlatformJsonCreate)?;
        serde_json::to_writer_pretty(platform_json, &self.platform_json)
            .map_err(PlatformLayerError::PlatformJsonWrite)?;

        // the computed env vars for this layer are written to this JSON file by the installer
        let layer_env_file_path = layer_path.join("layer_env.json"); // TODO: truncate?
        // a log of native packages not installed because of userland provides is written to this file
        let provided_packages_log_file_path = layer_path.join("provided_packages.tsv"); // TODO: truncate?

        let mut install_log = File::options()
            .create(true)
            .append(true) // guarantee multiple writers always append without race conditions
            .read(true)
            .open(layer_path.join("install.log"))
            .map_err(PlatformLayerError::InstallLogCreate)?
            .into_file();
        let outputs = install_log
            .try_clone()
            .map_err(PlatformLayerError::InstallLogCreate)?;
        let errors = outputs
            .try_clone()
            .map_err(PlatformLayerError::InstallLogCreate)?;

        let mut install_cmd = Command::new("composer");
        install_cmd
            .current_dir(layer_path)
            .envs(self.command_env) // we're invoking 'composer' from the bootstrap layer
            .args(["install", "--no-dev", "--no-interaction", "--no-progress"])
            .env("layer_env_file_path", &layer_env_file_path)
            .env(
                "providedextensionslog_file_path",
                &provided_packages_log_file_path,
            )
            .env("NO_COLOR", "1")
            .env("PHP_PLATFORM_INSTALLER_DISPLAY_OUTPUT_FDNO", "10")
            .env("PHP_PLATFORM_INSTALLER_DISPLAY_OUTPUT_INDENT", "2")
            .stdout(outputs)
            .stderr(errors);
        install_cmd
            .fd_mappings(vec![FdMapping {
                parent_fd: std::io::stdout()
                    .as_fd()
                    .try_clone_to_owned()
                    .map_err(PlatformLayerError::OutputFdSetup)?,
                child_fd: 10,
            }])
            .map_err(PlatformLayerError::OutputFdMapping)?;

        let status = install_cmd
            .status()
            .map_err(PlatformLayerError::ComposerInvocation)?;

        if !status.success() {
            let mut output = String::new();
            install_log
                .rewind()
                .map_err(PlatformLayerError::InstallLogRead)?;
            install_log
                .read_to_string(&mut output)
                .map_err(PlatformLayerError::InstallLogRead)?;
            return Err(PhpBuildpackError::PlatformLayer(
                PlatformLayerError::ComposerInstall(status, output),
            ));
        }

        // FIXME: we have to do that now, not later, since the installer gets invoked again
        // ^ to be solved on the installer side, which has to merge the values from later calls...

        // our platform installer plugin writes out a JSON file with env vars that we have to set
        // this is because many packages add to the path, set env var defaults, etc, and we cannot hard-code those in here
        // the values are pre-assembled in case of prepend/append, since only a single value may be set per env var in these cases
        let layer_env_values: Vec<LayerEnvValue> = serde_json::from_reader(BufReader::new(
            File::open(&layer_env_file_path).map_err(PlatformLayerError::ReadLayerEnv)?,
        ))
        .map_err(PlatformLayerError::ParseLayerEnv)?;

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

        // not all native packages (typically PHP extensions) might have gotten installed due to userland provide declarations
        // we now go over the "log" generated by the installer plugin and read the provides
        // then we attempt to force-install the provided packages as native variants
        // format is lines of fields separated by spaces (FIXME: tabs):
        // - first is name of "provider" package
        // - remaining fields are the native packages it claimed as provided
        // the great thing is that we process these install attempts in the order the solver originally produced them
        // which means that we install more "important" packages, that later ones might depend on, first
        // that, in turn, is important for loading order into PHP
        if let Ok(mut rdr) = csv::ReaderBuilder::new()
            .delimiter(b' ') // TODO: switch to tabs here and in installer plugin?
            .has_headers(false)
            .flexible(true) // variable number of "fields"
            .from_path(&provided_packages_log_file_path)
        {
            for result in rdr.deserialize() {
                let (provider_name, provides): (String, Vec<String>) =
                    result.map_err(PlatformLayerError::ProvidedPackagesLogRead)?;
                print::sub_bullet(format!(
                    "Attempting native package installs for {provider_name}"
                ));

                for provide in provides {
                    let (name, _version) = provide
                        .split_once(':')
                        .ok_or(PlatformLayerError::ProvidedPackagesLogParse)?;
                    let outputs = install_log
                        .try_clone()
                        .map_err(PlatformLayerError::InstallLogCreate)?;
                    let errors = outputs
                        .try_clone()
                        .map_err(PlatformLayerError::InstallLogCreate)?;
                    let mut install_cmd = Command::new("composer");
                    install_cmd
                        .current_dir(layer_path)
                        // .env("layer_env_file_path", &layer_env_file_path)
                        .envs(self.command_env) // we're invoking 'composer' from the bootstrap layer
                        .args([
                            "require",
                            &format!("{name}.native:*"),
                            "--no-interaction",
                            "--no-progress",
                        ])
                        .env("NO_COLOR", "1")
                        .env("PHP_PLATFORM_INSTALLER_DISPLAY_OUTPUT_FDNO", "10")
                        .env("PHP_PLATFORM_INSTALLER_DISPLAY_OUTPUT_INDENT", "4")
                        .stdout(outputs)
                        .stderr(errors);
                    install_cmd
                        .fd_mappings(vec![FdMapping {
                            parent_fd: std::io::stdout()
                                .as_fd()
                                .try_clone_to_owned()
                                .map_err(PlatformLayerError::OutputFdSetup)?,
                            child_fd: 10,
                        }])
                        .map_err(PlatformLayerError::OutputFdMapping)?;
                    if !install_cmd
                        .status()
                        .map_err(PlatformLayerError::ComposerInvocation)?
                        .success()
                    {
                        // the 'composer install' call was not successful, which means there was no "{name}:native" package available
                        print::plain(format!(
                            "    - No suitable native version of {name} available"
                        ));
                    }
                }
            }
        }

        let layer_metadata = generate_layer_metadata(&context.target);
        LayerResultBuilder::new(layer_metadata)
            .env(layer_env)
            .build()
    }
}

fn generate_layer_metadata(target: &Target) -> PlatformLayerMetadata {
    PlatformLayerMetadata {
        arch: target.arch.clone(),
        distro_name: target.distro_name.clone(),
        distro_version: target.distro_version.clone(),
    }
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug)]
pub(crate) enum PlatformLayerError {
    PlatformJsonCreate(std::io::Error),
    PlatformJsonWrite(serde_json::Error),
    InstallLogCreate(std::io::Error),
    OutputFdSetup(std::io::Error),
    OutputFdMapping(FdMappingCollision),
    ComposerInvocation(std::io::Error),
    InstallLogRead(std::io::Error),
    ComposerInstall(std::process::ExitStatus, String),
    ProvidedPackagesLogRead(csv::Error),
    ProvidedPackagesLogParse,
    ReadLayerEnv(std::io::Error),
    ParseLayerEnv(serde_json::Error),
}

impl From<PlatformLayerError> for PhpBuildpackError {
    fn from(error: PlatformLayerError) -> Self {
        Self::PlatformLayer(error)
    }
}
