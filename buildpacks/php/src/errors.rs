pub(crate) mod notices;

use crate::PhpBuildpackError;
use crate::layers::bootstrap::BootstrapLayerError;
use crate::layers::composer_env::ComposerEnvLayerError;
use crate::layers::platform::PlatformLayerError;
use crate::package_manager::composer::{
    ComposerLockVersionError, DependencyInstallationError, PlatformExtractorError,
    PlatformFinalizerError,
};
use crate::php_project::{PlatformJsonError, ProjectLoadError};
use crate::platform::generator::{
    ComposerRepositoryFromRepositoryUrlError, PlatformGeneratorError,
};
use crate::platform::{PlatformRepositoryUrlError, WebserversJsonError};
use crate::utils::DownloadUnpackError;
use bullet_stream::global::print;
use const_format::formatcp;
use indoc::{formatdoc, indoc};
use serde_json::error::Category;
use std::io;
use std::os::unix::process::ExitStatusExt;

#[rustfmt::skip]
pub(crate) const INTERNAL_ERROR_HELP_STRING: &str = {"\
Please make sure that you are not running an outdated version of the buildpack.

If the problem persists, open a support ticket and include the full log output of this build.\
"};

#[rustfmt::skip]
const TRANSIENT_ERROR_HELP_STRING: &str = formatcp! {"\
This is most likely a transient internal error; you should re-try the operation.

{INTERNAL_ERROR_HELP_STRING}\
"};

#[rustfmt::skip]
const LOCK_ERROR_HELP_STRING: &str = {"\
You most likely created or edited the file by hand, or a merge
conflict was not resolved properly, resulting in a syntax error
in the file. Refer to the docs for information on re-generating
the lock file: https://getcomposer.org/doc/01-basic-usage.md

Please perform the following steps locally on your computer to
resolve this issue before attempting another deploy:
1) Run 'composer update' to re-generate the lock file
2) stage the lock file changes using 'git add'
3) commit the change using 'git commit'

You can run 'composer validate' locally on your computer for
further diagnosis. Remember to also always keep your lock file
up to date with any changes according to the instructions at
https://getcomposer.org/doc/01-basic-usage.md\
"};

impl From<PhpBuildpackError> for libcnb::Error<PhpBuildpackError> {
    fn from(error: PhpBuildpackError) -> Self {
        Self::BuildpackError(error)
    }
}

fn format_io_error(e: &io::Error) -> String {
    format!("I/O Error: {e}")
}

fn format_serde_error(e: &serde_json::Error) -> String {
    let description = match e.classify() {
        Category::Io => "An I/O error occurred during parsing.",
        Category::Syntax => "A JSON syntax error was encountered.",
        Category::Eof => "Unexpected end of file.",
        Category::Data => "The parsed contents were invalid.",
    };
    formatdoc! {"
        {description}

        Details: {e}"
    }
}

impl PhpBuildpackError {
    pub(crate) fn on_error(self) {
        let (heading, message) = match self {
            PhpBuildpackError::ProjectLoad(e) => on_project_load_error(e),
            PhpBuildpackError::BootstrapLayer(e) => on_bootstrap_layer_error(e),
            PhpBuildpackError::PlatformRepositoryUrl(e) => match e {
                PlatformRepositoryUrlError::Split(e) => (
                    "Failed to parse platform repositories URL list".to_string(),
                    e.to_string(),
                ),
                PlatformRepositoryUrlError::Parse(e) => (
                    "Failed to parse platform repository URL".to_string(),
                    e.to_string(),
                ),
            },
            PhpBuildpackError::PlatformJson(e) => on_platform_json_error(e),
            PhpBuildpackError::WebserversJson(e) => match e {
                WebserversJsonError::PlatformGenerator(e) => on_platform_generator_error(e),
            },
            PhpBuildpackError::PlatformLayer(e) => on_platform_layer_error(e),
            PhpBuildpackError::DependencyInstallation(e) => on_dependency_installation_error(e),
            PhpBuildpackError::ComposerEnvLayer(e) => on_composer_env_layer_error(e),
        };
        print::error(formatdoc! {"
            {heading}

            {message}
        "});
    }
}

fn on_project_load_error(e: ProjectLoadError) -> (String, String) {
    match e {
        ProjectLoadError::ComposerLockRead(filename, e)
        | ProjectLoadError::ComposerJsonRead(filename, e) => {
            (format!("Failed to read '{filename}'"), format_io_error(&e))
        }
        ProjectLoadError::ComposerJsonParse(filename, e) => (
            format!("Failed to parse '{filename}'"),
            formatdoc! {"
                {message}

                Please run 'composer validate' on your local computer for verification.

                If you believe this message to be in error, please report it.",
                message = format_serde_error(&e)
            },
        ),
        ProjectLoadError::ComposerLockParse(filename, e) => (
            format!("Failed to parse '{filename}'"),
            formatdoc! {"
                There was an error parsing the lock file; it must be a valid
                file generated by Composer and be in an up-to-date state.

                Check below for any parse errors and address them if necessary.

                {serde_error}

                {LOCK_ERROR_HELP_STRING}

                If you believe this message to be in error, please report it.",
                serde_error = format_serde_error(&e)
            },
        ),
        ProjectLoadError::ComposerLockMissing(json_name, lock_name) => (
            "No Composer lock file found".to_string(),
            formatdoc! {"
                A '{lock_name}' file was not found in your project, but there
                is a '{json_name}' file with dependencies inside 'require'.

                The lock file is required in order to guarantee reliable and
                reproducible installation of dependencies across platforms and
                deploys. You must follow the Composer best practice of having
                your lock file under version control in order to deploy. The
                lock file must not be in your '.gitignore'.

                Please perform the following steps locally on your computer to
                resolve this issue before attempting another deploy:
                1) remove '{lock_name}' from file '.gitignore', if present
                2) if no '{lock_name}' exists, run 'composer update'
                3) stage the lock file changes using 'git add {lock_name}'
                4) if you edited '.gitignore', also run 'git add .gitignore'
                5) commit the change using 'git commit'

                Please remember to always keep your '{lock_name}' updated in
                lockstep with '{json_name}' to avoid common problems related
                to dependencies during collaboration and deployment.

                Please refer to the Composer documentation for further details:
                https://getcomposer.org/doc/
                https://getcomposer.org/doc/01-basic-usage.md
            "},
        ),
    }
}

fn on_bootstrap_layer_error(e: BootstrapLayerError) -> (String, String) {
    let (heading, message) = match e {
        BootstrapLayerError::DownloadUnpack(e) => match e {
            DownloadUnpackError::Io(e) => (
                "An I/O error occurred during bootstrapping".to_string(),
                format_io_error(&e),
            ),
            DownloadUnpackError::Request(e) => (
                "A download error occurred during bootstrapping".to_string(),
                e.to_string(),
            ),
        },
    };
    (
        heading,
        formatdoc! {"
            {message}

            {TRANSIENT_ERROR_HELP_STRING}",
            message = message.trim()
        },
    )
}

fn on_platform_layer_error(e: PlatformLayerError) -> (String, String) {
    match e {
        PlatformLayerError::PlatformJsonCreate(e)
        | PlatformLayerError::InstallLogCreate(e)
        | PlatformLayerError::ComposerInvocation(e)
        | PlatformLayerError::InstallLogRead(e)
        | PlatformLayerError::ReadLayerEnv(e) => (
            "An I/O error occurred during platform packages installation".to_string(),
            formatdoc! {"
                Details: {e}

                {INTERNAL_ERROR_HELP_STRING}
            "},
        ),
        PlatformLayerError::PlatformJsonWrite(e) => (
            "Failed to write platform dependencies file".to_string(),
            formatdoc! {"
                Details: {e}

                {INTERNAL_ERROR_HELP_STRING}
            "},
        ),
        PlatformLayerError::ProvidedPackagesLogRead(e) => (
            "Failed to read platform installer packages log".to_string(),
            formatdoc! {"
                Details: {e}

                {INTERNAL_ERROR_HELP_STRING}
            "},
        ),
        PlatformLayerError::ProvidedPackagesLogParse => (
            "Failed to parse platform installer packages log".to_string(),
            INTERNAL_ERROR_HELP_STRING.to_string(),
        ),
        PlatformLayerError::ComposerInstall(exit_status, output) => (
            "Failed to install platform dependencies".to_string(),
            match &exit_status.code() {
                Some(2) => formatdoc! {"
                    Your platform requirements (for runtimes and extensions) could
                    not be resolved to an installable set of dependencies.

                    This usually means that you (or packages you are using) depend
                    on a combination of PHP versions and/or extensions that are
                    currently not available on Heroku.

                    The following is the full output from the installation attempt:

                    {output}

                    Please verify that all requirements for runtime versions in
                    'composer.lock' are compatible with the list below, and ensure
                    all required extensions are available for the desired runtimes.

                    When choosing a PHP runtimes and extensions, please also ensure
                    they are available on your app's stack, and, if necessary, choose
                    a different stack after consulting the article below.

                    For a list of supported runtimes & extensions on Heroku, please
                    refer to: https://devcenter.heroku.com/articles/php-support
                "},
                Some(exit_code) => formatdoc! {"
                    An error ({exit_code}) occurred during installation.

                    The following is the full output from the installation attempt:

                    {output}
                "},
                None => formatdoc! {"
                    The operation was terminated (signal: {signal})

                    Output until termination:

                    {output}
                    ",
                    signal = &exit_status.signal().unwrap_or(-1)
                },
            },
        ),
        PlatformLayerError::ParseLayerEnv(e) => (
            "Failed to read platform installer layer env output".to_string(),
            formatdoc! {"
                Details: {e}

                {INTERNAL_ERROR_HELP_STRING}
            "},
        ),
    }
}

fn on_platform_json_error(e: PlatformJsonError) -> (String, String) {
    match e {
        PlatformJsonError::Extractor(e) => match e {
            PlatformExtractorError::ComposerLockVersion(e) => match e {
                ComposerLockVersionError::InvalidPlatformApiVersion(version) => (
                    "Invalid 'platform-api-version' in lock file".to_string(),
                    format!("Bad version number '{version}' in lock file."),
                ),
            },
        },
        PlatformJsonError::Generator(e) => on_platform_generator_error(e),
        PlatformJsonError::Finalizer(e) => match e {
            PlatformFinalizerError::RuntimeRequirementInRequireDevButNotRequire => (
                "Runtime specified in 'require-dev' but not in 'require'".to_string(),
                indoc! {"
                    Your 'composer.json' contains a 'require-dev' section which
                    specifies a PHP runtime version (either directly, or through
                    a dependency), but no such requirement is present in 'require'
                    or in any of the packages listed in 'require'.

                    Even if dev requirements are not being installed, the entirety
                    of all dependencies needs to resolve to an installable set.
                    Heroku cannot select a default runtime version in this case.

                    Please perform the following steps locally on your computer to
                    resolve this issue before attempting another deploy:
                    1) add a dependency for 'php' to 'require' in 'composer.json'
                    2) run 'composer update' to re-generate the lock file
                    3) stage changes using 'git add composer.json composer.lock'
                    4) commit changes using 'git commit'

                    For more information on selecting PHP runtimes, please refer to
                    https://devcenter.heroku.com/articles/php-support
                "}
                .to_string(),
            ),
        },
    }
}

fn on_platform_generator_error(e: PlatformGeneratorError) -> (String, String) {
    match e {
        PlatformGeneratorError::EmptyPlatformRepositoriesList => (
            "No platform repositories configured".to_string(),
            indoc! {"
                Your configured list of platform package repositories ended up empty.
                Ensure that the last entry in your list is not '-', which is the list token
                that re-sets the list to empty.
            "}
            .to_string(),
        ),
        PlatformGeneratorError::FromRepositoryUrl(e) => match e {
            ComposerRepositoryFromRepositoryUrlError::MultipleFilters => (
                "Conflicting filters in platform repository URL".to_string(),
                indoc! {"
                    One of your platform package repository URLs contains filters arguments for both
                    exclusive and inclusive filtering of packages. Please use adjust the URL to only
                    contain one type of filter.
                "}
                .to_string(),
            ),
        },
        PlatformGeneratorError::InvalidStackIdentifier(stack) => (
            "Invalid stack identifier".to_string(),
            format!("Stack name '{stack}' does not follow allowed '$NAME-$VERSION' pattern."),
        ),
    }
}

fn on_dependency_installation_error(e: DependencyInstallationError) -> (String, String) {
    match e {
        DependencyInstallationError::ComposerInvocation(e) => (
            "An I/O error occurred during dependency installation".to_string(),
            formatdoc! {"
                Details: {e}

                {INTERNAL_ERROR_HELP_STRING}
            "},
        ),
        DependencyInstallationError::ComposerInstall(exit_status) => (
            "Dependency installation failed!".to_string(),
            formatdoc! {"
                The 'composer install' process failed with status {exit_code}. The cause
                may be the download or installation of packages, or a pre- or
                post-install hook (e.g. a 'post-install-cmd' item in 'scripts')
                in your 'composer.json'.

                Typical error cases are out-of-date or missing parts of code,
                timeouts when making external connections, or memory limits.

                Check the above error output closely to determine the cause of
                the problem, ensure the code you're pushing is functioning
                properly, and that all local changes are committed correctly.

                For more information on builds for PHP on Heroku, refer to
                https://devcenter.heroku.com/articles/php-support
                ", exit_code = exit_status.code().unwrap_or(-1)
            },
        ),
    }
}

fn on_composer_env_layer_error(e: ComposerEnvLayerError) -> (String, String) {
    match e {
        ComposerEnvLayerError::ConfigBinDirCmd(cmd_error) => (
            "Could not determine Composer 'bin-dir' config value".to_string(),
            formatdoc! {"
                Without this value, the buildpack cannot place the binaries installed by composer on the PATH,
                which is needed to run the application. The buildpack cannot continue.

                Error details:

                {cmd_error}

                {INTERNAL_ERROR_HELP_STRING}
            "},
        ),
    }
}
