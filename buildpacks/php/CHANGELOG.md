# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- PHP/8.3.30
- PHP/8.4.17
- PHP/8.5.2
- ext-phalcon/5.10.0
- ext-amqp/2.2.0
- Nginx/1.28.1
- Composer/2.2.26
- Composer/2.9.3
- librdkafka/2.13.0

## [1.3.1] - 2025-12-19

### Added

- PHP/8.1.33
- PHP/8.2.30
- PHP/8.3.29
- PHP/8.4.16
- PHP/8.5.1
- ext-apcu/5.1.28

### Changed

- Apache/2.4.66

### Fixed

- Composer platform installer plugin produces (invisible) PHP 8.4 deprecation notice

## [1.3.0] - 2025-12-12

### Added

- ext-oauth/2.0.10
- ext-imagick/3.8.1
- PHP/8.5.0

### Changed

- Use PHP 8.4 for bootstrapping

## [1.2.0] - 2025-11-22

### Added

- PHP/8.3.28
- PHP/8.4.15
- ext-ev/1.2.2
- ext-redis/6.3.0
- Composer/2.9.2

### Changed

- Filter output from Composer on platform installation failure for better readability (for parity with the Classic buildpack)
- Explicitly handle newly introduced `name` field in `composer.json` (version 2.9+) `repositories` definitions

## [1.1.0] - 2025-10-27

### Changed

- Drop use of Classic buildpack repository (for Composer installer plugin and web server boot scripts and configs) during platform packages installation
- Output filtered, "human-readable" progress info during platform packages installation

## [1.0.10] - 2025-10-24

### Added

- PHP/8.3.27
- PHP/8.4.14
- ext-mongodb/1.21.2
- ext-mongodb/2.1.4
- ext-memcached/3.4.0
- ext-grpc/1.76.0
- librdkafka/2.12.1

## [1.0.9] - 2025-09-30

### Added

- PHP/8.3.26
- PHP/8.4.13
- ext-grpc/1.75.0
- Composer/2.8.12

## [1.0.8] - 2025-09-04

### Added

- PHP/8.3.25
- PHP/8.4.12
- ext-apcu/5.1.27
- ext-raphf/2.0.2
- librdkafka/2.11.1
- Composer/2.8.11

## [1.0.7] - 2025-08-14

### Changed

- Retry downloads during bootstrapping ([#226](https://github.com/heroku/buildpacks-php/pull/226))
- Consistent handling of "non-URL" URLs (SSH-style or path) in `composer.json`/`composer.lock` repositories and package `source`s/`dist`s ([#105](https://github.com/heroku/buildpacks-php/issue/105), [#187](https://github.com/heroku/buildpacks-php/issue/187), [#208](https://github.com/heroku/buildpacks-php/pull/208))
- Support object-style notation of repositories in composer.json ([#209](https://github.com/heroku/buildpacks-php/pull/209))

## [1.0.6] - 2025-07-31

### Added

- PHP/8.3.24
- PHP/8.4.11
- ext-grpc/1.74.0
- ext-apcu/5.1.25
- Composer/2.8.10
- Apache/2.4.65

## [1.0.5] - 2025-07-04

### Added

- PHP/8.1.33
- PHP/8.2.29
- PHP/8.3.23
- PHP/8.4.10
- ext-mongodb/2.1.1
- ext-mongodb/1.21.1
- librdkafka/2.11.0

## [1.0.4] - 2025-06-13

### Added

- PHP/8.3.22
- PHP/8.4.8
- ext-mongodb/2.1.0
- ext-grpc/1.73.0
- librdkafka/2.10.1

## [1.0.3] - 2025-05-15

### Added

- PHP/8.3.21
- PHP/8.4.7
- ext-phalcon/5.9.3
- ext-grpc/1.72.0
- ext-uuid/1.3.0
- librdkafka/2.10.0
- Nginx/1.28.0
- Composer/2.8.9

### Fixed

- Nginx fails to start ([#186](https://github.com/heroku/buildpacks-php/issues/186))

## [1.0.2] - 2025-05-13

### Changed

- Drop support for heroku-20 ([#197](https://github.com/heroku/buildpacks-php/pull/197))
- Use repository snapshots for platform packages ([#197](https://github.com/heroku/buildpacks-php/pull/197))

### Fixed

- Installation of multiple "polyfill" packages fails due to reused Command struct ([#197](https://github.com/heroku/buildpacks-php/pull/197))

## [1.0.1] - 2025-04-29

### Fixed

- Errors from command executions now include the command being run in addition to the exit status. ([#180](https://github.com/heroku/buildpacks-php/pull/180))

## [1.0.0] - 2025-04-28

### Changed

- Update build output style. ([#171](https://github.com/heroku/buildpacks-php/pull/171))

## [0.2.4] - 2025-04-10

### Fixed

- Fix `composer.lock` parsing when "dist" key contains a `"type": "path"`. ([#176](https://github.com/heroku/buildpacks-php/pull/176))
- All raw file system errors now include the filenames via the `fs-err` crate. ([#174](https://github.com/heroku/buildpacks-php/pull/174))

## [0.2.3] - 2025-04-08

### Fixed

- The "scripts" key in `composer.json` no longer fails when provided with an object as a sub-value. ([#168](https://github.com/heroku/buildpacks-php/pull/168))

## [0.2.2] - 2025-04-03

### Changed

- Updated libcnb to 0.28.1, which includes tracing improvements/fixes. ([#165](https://github.com/heroku/buildpacks-php/pull/165))

## [0.2.1] - 2025-02-28

### Changed

- Enabled `libcnb`'s `trace` feature. ([#154](https://github.com/heroku/buildpacks-php/pull/154))

## [0.2.0] - 2024-06-04

### Added

- Add PHP/8.3, update PHP runtimes, extensions, Composers, web servers (#104)
- Support Ubuntu 24.04 (and, as a result, Heroku-24 and `heroku/builder:24`)
- Support `arm64` CPU architecture (Ubuntu 24.04 / Heroku-24 only)

### Changed

- Use Buildpack API 0.10 (requires `lifecycle` 0.17 or newer)
- `buildpack.toml` declaration of `[[stacks]]` has been replaced with `[[targets]]`, currently supporting Ubuntu 20.04 and 22.04 (both `amd64`)
- Bump versions of Composer and minimal PHP for bootstrapping to 2.7.6 and 8.3.7

### Fixed

- Strings should be allowed as values in `scripts` object in `composer.json` ([#90](https://github.com/heroku/buildpacks-php/issues/90))

## [0.1.2] - 2023-10-24

### Changed

- Updated buildpack display name and description. ([#57](https://github.com/heroku/buildpack-php/pull/57))

## [0.1.1] - 2023-06-30

### Added

- Initial implementation

[unreleased]: https://github.com/heroku/buildpacks-php/compare/v1.3.1...HEAD
[1.3.1]: https://github.com/heroku/buildpacks-php/compare/v1.3.0...v1.3.1
[1.3.0]: https://github.com/heroku/buildpacks-php/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/heroku/buildpacks-php/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/heroku/buildpacks-php/compare/v1.0.10...v1.1.0
[1.0.10]: https://github.com/heroku/buildpacks-php/compare/v1.0.9...v1.0.10
[1.0.9]: https://github.com/heroku/buildpacks-php/compare/v1.0.8...v1.0.9
[1.0.8]: https://github.com/heroku/buildpacks-php/compare/v1.0.7...v1.0.8
[1.0.7]: https://github.com/heroku/buildpacks-php/compare/v1.0.6...v1.0.7
[1.0.6]: https://github.com/heroku/buildpacks-php/compare/v1.0.5...v1.0.6
[1.0.5]: https://github.com/heroku/buildpacks-php/compare/v1.0.4...v1.0.5
[1.0.4]: https://github.com/heroku/buildpacks-php/compare/v1.0.3...v1.0.4
[1.0.3]: https://github.com/heroku/buildpacks-php/compare/v1.0.2...v1.0.3
[1.0.2]: https://github.com/heroku/buildpacks-php/compare/v1.0.1...v1.0.2
[1.0.1]: https://github.com/heroku/buildpacks-php/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/heroku/buildpacks-php/compare/v0.2.4...v1.0.0
[0.2.4]: https://github.com/heroku/buildpacks-php/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/heroku/buildpacks-php/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/heroku/buildpacks-php/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/heroku/buildpacks-php/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/heroku/buildpacks-php/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/heroku/buildpacks-php/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/heroku/buildpacks-php/releases/tag/v0.1.1
