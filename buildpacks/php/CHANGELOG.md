# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Stack `heroku-20` (Ubuntu 20.04) is no longer supported. ([#193](https://github.com/heroku/buildpacks-php/pull/193))

### Added

- PHP 8.4 and 8.3 security updates are now available. ([#193](https://github.com/heroku/buildpacks-php/pull/193))

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

[unreleased]: https://github.com/heroku/buildpacks-php/compare/v1.0.1...HEAD
[1.0.1]: https://github.com/heroku/buildpacks-php/compare/v1.0.0...v1.0.1
[1.0.0]: https://github.com/heroku/buildpacks-php/compare/v0.2.4...v1.0.0
[0.2.4]: https://github.com/heroku/buildpacks-php/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/heroku/buildpacks-php/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/heroku/buildpacks-php/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/heroku/buildpacks-php/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/heroku/buildpacks-php/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/heroku/buildpacks-php/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/heroku/buildpacks-php/releases/tag/v0.1.1
