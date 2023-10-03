use libcnb_test::{
    assert_contains, BuildConfig, BuildpackReference, ContainerConfig, TestContext, TestRunner,
};
use std::env;
use std::path::Path;
use std::time::Duration;

/// Helper for testing containers that expose a HTTP interface.
///
/// It will start the container from a `TestContext`, sets the `PORT` environment variable and tries
/// to get a successful (HTTP 200) response from the container for a `GET` request to `/`. It will
/// then assert that the given string is contained in the request body.
///
/// This helper will retry failed requests with an exponential backoff to avoid flappy tests.
///a
/// The smoke integration tests need to ensure the container runs as expected.
/// This function is catering to that use-case and is not useful in other contexts.
pub(crate) fn start_container_assert_basic_http_response(
    context: &TestContext,
    expected_http_response_body_contains: &str,
) {
    context.start_container(
        ContainerConfig::default()
            .expose_port(PORT)
            .env("PORT", PORT.to_string()),
        |context| {
            let url = format!("http://{}", context.address_for_port(PORT));

            let response_body = http_request_backoff(|| ureq::get(&url).call())
                .expect(UREQ_RESPONSE_RESULT_EXPECT_MESSAGE)
                .into_string()
                .expect(UREQ_RESPONSE_AS_STRING_EXPECT_MESSAGE);

            assert_contains!(&response_body, expected_http_response_body_contains);
        },
    );
}

pub(crate) fn http_request_backoff<F, T, E>(request_fn: F) -> Result<T, E>
where
    F: Fn() -> Result<T, E>,
{
    let backoff =
        exponential_backoff::Backoff::new(32, Duration::from_secs(1), Duration::from_secs(5 * 60));

    let mut backoff_durations = backoff.into_iter();

    loop {
        match request_fn() {
            result @ Ok(_) => return result,
            result @ Err(_) => match backoff_durations.next() {
                None => return result,
                Some(backoff_duration) => {
                    std::thread::sleep(backoff_duration);
                    continue;
                }
            },
        }
    }
}

/// Helper for smoke-testing.
///
/// Builds the app with the given buildpacks, asserts that the build finished successfully, and
/// builds the app again to ensure that any caching logic does not break subsequent builds.
/// After the build, an HTTP request is made, asserting that the given string is in the response.
pub(crate) fn smoke_test<P, B>(
    builder_name: impl AsRef<str>,
    app_dir: P,
    buildpacks: B,
    expected_http_response_body_contains: &str,
) where
    P: AsRef<Path>,
    B: Into<Vec<BuildpackReference>>,
{
    let build_config = BuildConfig::new(builder_name.as_ref(), app_dir)
        .buildpacks(buildpacks.into())
        .clone();

    TestRunner::default().build(&build_config, |context| {
        start_container_assert_basic_http_response(&context, expected_http_response_body_contains);

        context.rebuild(&build_config, |context| {
            start_container_assert_basic_http_response(
                &context,
                expected_http_response_body_contains,
            );
        });
    });
}

pub const DEFAULT_INTEGRATION_TEST_BUILDER: &str = "heroku/builder:22";

pub const UREQ_RESPONSE_RESULT_EXPECT_MESSAGE: &str = "http request should be successful";

pub const UREQ_RESPONSE_AS_STRING_EXPECT_MESSAGE: &str =
    "http response body should be convertible to a string";

const PORT: u16 = 8080;

pub(crate) fn builder() -> String {
    env::var("INTEGRATION_TEST_CNB_BUILDER").unwrap_or(DEFAULT_INTEGRATION_TEST_BUILDER.to_string())
}

pub(crate) fn default_buildpacks() -> Vec<BuildpackReference> {
    vec![
        BuildpackReference::CurrentCrate,
        // Using an explicit version from Docker Hub to prevent failures when there
        // are multiple Procfile buildpack versions in the builder image.
        BuildpackReference::Other(String::from("docker://docker.io/heroku/procfile-cnb:2.0.1")),
    ]
}
