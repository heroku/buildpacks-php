//! Bundle all integration tests into one binary to:
//! - Reduce compile times
//! - Reduce required disk space
//! - Increase parallelism
//!
//! See: <https://matklad.github.io/2021/02/27/delete-cargo-integration-tests.html#Implications>

#![warn(clippy::pedantic)]

mod smoke;
mod utils;

use libcnb_test::BuildpackReference;

pub(crate) fn default_buildpacks() -> Vec<BuildpackReference> {
    vec![
        BuildpackReference::Crate,
        // BuildpackReference::Other(String::from("heroku/procfile")),
    ]
}
