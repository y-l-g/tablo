# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.0](https://github.com/y-l-g/tablo/compare/tablo-test-v0.7.0...tablo-test-v0.8.0) - 2026-10-17

### Added

- *(table)* let each user toggle a list's columns ([#675](https://github.com/y-l-g/tablo/pull/675))
- *(panel)* [**breaking**] add a favicon to `Brand` and badges to sidebar entries ([#673](https://github.com/y-l-g/tablo/pull/673))

## [0.7.0](https://github.com/y-l-g/tablo/compare/tablo-test-v0.6.0...tablo-test-v0.7.0) - 2026-10-14

### Added

- *(table)* multi-select, date-range and relationship filters ([#617](https://github.com/y-l-g/tablo/pull/617))

### Changed

- *(test)* dedup TestClient helpers ([#635](https://github.com/y-l-g/tablo/pull/635))

### Fixed

- *(table)* [**breaking**] follow up the filter controls ([#617](https://github.com/y-l-g/tablo/pull/617)) ([#622](https://github.com/y-l-g/tablo/pull/622))

### Test

- [**breaking**] promote helpers into tablo-test, pin refusals with trybuild, assert kinds over wording ([#598](https://github.com/y-l-g/tablo/pull/598))

## [0.6.0](https://github.com/y-l-g/tablo/compare/tablo-test-v0.5.1...tablo-test-v0.6.0) - 2026-10-10

### Added

- *(test)* sign a test client in with TestClient::sign_in ([#555](https://github.com/y-l-g/tablo/pull/555))

### Other

- reorganize the suite by feature and pin each behavior once ([#552](https://github.com/y-l-g/tablo/pull/552))

## [0.4.0](https://github.com/y-l-g/tablo/compare/tablo-test-v0.3.0...tablo-test-v0.4.0) - 2026-10-07

### Other

- *(repo)* keep the rules in two short files and the ADRs to their decisions ([#525](https://github.com/y-l-g/tablo/pull/525))
- *(core)* [**breaking**] keep Topcoat's runtime as the only browser layer ([#516](https://github.com/y-l-g/tablo/pull/516))
- *(repo)* [**breaking**] home the test protocol helpers in tablo-test ([#509](https://github.com/y-l-g/tablo/pull/509))

## [0.3.0](https://github.com/y-l-g/tablo/compare/tablo-test-v0.2.0...tablo-test-v0.3.0) - 2026-10-06

### Fixed

- *(repo)* home the protocol helpers in tablo-core ([#493](https://github.com/y-l-g/tablo/pull/493))

### Other

- *(repo)* add a README to every published crate ([#503](https://github.com/y-l-g/tablo/pull/503))

## [0.2.0](https://github.com/y-l-g/tablo/compare/tablo-test-v0.1.0...tablo-test-v0.2.0) - 2026-10-06

### Fixed

- *(repo)* inherit the repository URL in every crate ([#484](https://github.com/y-l-g/tablo/pull/484))
