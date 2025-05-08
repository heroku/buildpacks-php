use crate::package_manager::composer;
use crate::platform::generator;
use crate::tests::platform::ComposerLockTestCaseConfig;
use assert_json_diff::{assert_json_matches_no_panic, CompareMode, Config};
use figment::providers::{Format, Serialized, Toml};
use figment::Figment;
use fs_err as fs;
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[test]
fn make_platform_json_with_fixtures() {
    let installer_path = &PathBuf::from("../../support/installer");

    let cases = fs::read_dir(Path::new("tests/fixtures/platform/generator"))
        .unwrap()
        .filter(|der| der.as_ref().unwrap().metadata().unwrap().is_dir())
        .filter_map(|der| {
            let p = der.unwrap().path();
            // merge our auto-built config (from Path) and a config.toml, if it exists
            let case: ComposerLockTestCaseConfig =
                Figment::from(Serialized::defaults(ComposerLockTestCaseConfig::from(&p)))
                    .merge(Toml::file(p.join("config.toml")))
                    .extract()
                    .unwrap();
            // skip if there isn't even a lock file in the dir
            case.lock.is_some().then_some(case)
        });

    // Prints the original message on failure
    std::panic::set_hook(Box::new(|info| {
        eprintln!("Failed: {info}");
    }));

    let mut failed_cases = Vec::new();
    for case in cases {
        let name = case.name.clone().map_or("<unknown>".to_string(), |v| v);
        // Discards the panic, logs the failed count and continues
        if std::panic::catch_unwind(|| assert_case(installer_path, case)).is_err() {
            failed_cases.push(format!("- `{name}`"));
        }
    }

    // If 1 or more failures occur, fails the test so all error messages are visible
    #[allow(clippy::manual_assert)]
    if !failed_cases.is_empty() {
        panic!(
            "Failed {} test case(s):\n\n{}",
            failed_cases.len(),
            failed_cases.join("\n")
        )
    }
}

#[allow(clippy::too_many_lines)]
fn assert_case(installer_path: &Path, case: ComposerLockTestCaseConfig) {
    let lock = serde_json::from_str(
        // .relative() will allow specifying the file name in the config
        &fs::read_to_string(case.lock.as_ref().unwrap().relative()).unwrap(),
    )
    .unwrap();

    // FIRST: from the lock file, extract a generator config and packages list

    let generator_input = composer::extract_from_lock(&lock);

    // first check: was this even supposed to succeed or fail?
    assert_eq!(
        generator_input.is_ok(),
        case.expect_extractor_failure.is_none(),
        "case {}: lock extraction expected to {}, but it didn't",
        case.name.as_ref().unwrap(),
        if generator_input.is_ok() {
            "fail"
        } else {
            "succeed"
        },
    );

    // on failure, check if the type of failure what was the test expected
    let mut extractor_notices = Vec::<composer::PlatformExtractorNotice>::new();
    let generator_input = match generator_input {
        Ok(v) => v.unwrap(&mut extractor_notices),
        Err(e) => {
            assert!(
                        case.expect_extractor_failure.is_some(),
                        "case {}: lock extraction failed, but config has no expect_extractor_failure type specified",
                        case.name.as_ref().unwrap()
                    );

            assert_eq!(
                format!("{e:?}"),
                case.expect_extractor_failure.unwrap(),
                "case {}: lock extraction failed as expected, but with mismatched failure type",
                case.name.as_ref().unwrap()
            );

            return;
        }
    };

    // fetch all notices and compare them against the list of expected notices
    assert_eq!(
        extractor_notices
            .iter()
            .map(|v| format!("{v:?}"))
            .collect::<HashSet<String>>(),
        case.expected_extractor_notices
            .unwrap_or_default()
            .into_iter()
            .collect::<HashSet<String>>(),
        "case {}: mismatched lock extractor notices (left = generated, right = expected)",
        case.name.as_ref().unwrap()
    );

    // SECOND: generate "platform.json" from the extracted config and packages list

    let generated_json_package = generator::generate_platform_json(
        &generator_input,
        &case.stack,
        installer_path,
        &case.repositories,
    );

    // first check: was this even supposed to succeed or fail?
    assert_eq!(
        generated_json_package.is_ok(),
        case.expect_generator_failure.is_none(),
        "case {}: generation expected to {}, but it didn't",
        case.name.as_ref().unwrap(),
        if generated_json_package.is_ok() {
            "fail"
        } else {
            "succeed"
        },
    );

    // on failure, check if the type of failure what was the test expected
    let mut generated_json_package = match generated_json_package {
        Ok(v) => v,
        Err(e) => {
            assert!(
                        case.expect_generator_failure.is_some(),
                        "case {}: generation failed, but config has no expect_generator_failure type specified",
                        case.name.as_ref().unwrap()
                    );

            assert_eq!(
                format!("{e:?}"),
                case.expect_generator_failure.unwrap(),
                "case {}: generation failed as expected, but with mismatched failure type",
                case.name.as_ref().unwrap()
            );

            return;
        }
    };

    // THIRD: post-process the generated result to ensure/validate runtime requirements etc
    let ensure_runtime_requirement_result =
        composer::ensure_runtime_requirement(&mut generated_json_package);

    // first check: was this even supposed to succeed or fail?
    assert_eq!(
        ensure_runtime_requirement_result.is_ok(),
        case.expect_finalizer_failure.is_none(),
        "case {}: finalizing expected to {}, but it didn't",
        case.name.as_ref().unwrap(),
        if ensure_runtime_requirement_result.is_ok() {
            "fail"
        } else {
            "succeed"
        },
    );

    // on failure, check if the type of failure what was the test expected
    let finalizer_notices = match ensure_runtime_requirement_result {
        Ok(v) => v,
        Err(e) => {
            assert!(
                        case.expect_finalizer_failure.is_some(),
                        "case {}: finalizing failed, but config has no expect_finalizer_failure type specified",
                        case.name.as_ref().unwrap()
                    );

            assert_eq!(
                format!("{e:?}"),
                case.expect_finalizer_failure.unwrap(),
                "case {}: finalizing failed as expected, but with mismatched failure type",
                case.name.as_ref().unwrap()
            );

            return;
        }
    };

    // fetch all notices and compare them against the list of expected notices
    assert_eq!(
            finalizer_notices
                .iter()
                .map(|v| format!("{v:?}"))
                .collect::<HashSet<String>>(),
            case.expected_finalizer_notices
                .unwrap_or_default()
                .into_iter()
                .collect::<HashSet<String>>(),
            "case `{name}`: mismatched finalizer notices. Update the `{name}/config.toml` or fix the behavior. (left = generated, right = expected)",
            name = case.name.as_ref().unwrap()
        );

    if !case.install_dev {
        // remove require-dev if we do not want dev installs
        generated_json_package.package.require_dev.take();
    }

    let mut expected_json_object: Map<String, Value> = serde_json::from_str(
        &fs::read_to_string(case.expected_result.unwrap().relative()).unwrap(),
    )
    .unwrap();

    let generated_json_value = serde_json::value::to_value(&generated_json_package).unwrap();
    let generated_json_object = generated_json_value.as_object().unwrap();

    let generated_keys: HashSet<String> = generated_json_object.keys().cloned().collect();
    let expected_keys: HashSet<String> = expected_json_object.keys().cloned().collect();

    // check if all of the expected keys are there (and only those)
    assert_eq!(
        &generated_keys,
        &expected_keys,
        "case {}: mismatched keys (left = generated, right = expected)",
        case.name.as_ref().unwrap()
    );

    // validate each key in the generated JSON
    // we have to do this because we want to treat e.g. the "provide" key a bit differently
    for key in expected_keys {
        let generated_value = generated_json_object.get(key.as_str()).unwrap();
        let expected_value = match key.as_str() {
            k @ "provide" => {
                if let Value::Object(obj) = &mut expected_json_object.get_mut(k).unwrap() {
                    // for heroku-sys/heroku, we want to check that the generated value starts with the expected value
                    // (since the version strings are like XX.YYYY.MM.DD, with XX being the stack version number)
                    obj.entry("heroku-sys/heroku").and_modify(|exp| {
                        let gen = generated_value.get("heroku-sys/heroku").unwrap();
                        if gen.as_str().unwrap().starts_with(exp.as_str().unwrap()) {
                            *exp = gen.clone();
                        }
                    });
                }
                expected_json_object.get(k).unwrap()
            }
            // k @ "repositories" => expected_json_object.get(k).unwrap(), // maybe normalize plugin repo path, maybe sort packages in "package" repo?
            k => expected_json_object.get(k).unwrap(),
        };

        let comparison = assert_json_matches_no_panic(
            generated_value,
            expected_value,
            Config::new(CompareMode::Strict),
        )
        .map_err(|err| format!("case {}, key {}: {}", case.name.as_ref().unwrap(), key, err));

        assert!(comparison.is_ok(), "{}", comparison.unwrap_err());
    }
}
