# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- Support composer scripts as objects to support symfony apps #132

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

[unreleased]: https://github.com/heroku/buildpacks-php/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/heroku/buildpacks-php/compare/v0.1.2...v0.2.0
[0.1.2]: https://github.com/heroku/buildpacks-php/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/heroku/buildpacks-php/releases/tag/v0.1.1
