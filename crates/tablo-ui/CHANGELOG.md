# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0](https://github.com/y-l-g/tablo/compare/tablo-ui-v0.5.1...tablo-ui-v0.6.0) - 2026-10-10

### Added

- *(schema)* link records through a join model, from a form and a related table ([#576](https://github.com/y-l-g/tablo/pull/576))
- *(schema)* edit a #[document] list in a repeater, a row per item ([#573](https://github.com/y-l-g/tablo/pull/573))
- *(panel)* ask for an action's input in a dialog over the page ([#572](https://github.com/y-l-g/tablo/pull/572))
- *(schema)* narrow a relationship choice to the rows of another field's value ([#571](https://github.com/y-l-g/tablo/pull/571))
- *(panel)* [**breaking**] run actions from record pages, list headers and custom pages ([#567](https://github.com/y-l-g/tablo/pull/567)) ([#568](https://github.com/y-l-g/tablo/pull/568))
- *(panel)* export the list as CSV from the page header ([#547](https://github.com/y-l-g/tablo/pull/547))

### Other

- *(ui)* say why selects.js is a script, citing tokio-rs/topcoat#504 ([#574](https://github.com/y-l-g/tablo/pull/574))
- compile every rustdoc example ([#554](https://github.com/y-l-g/tablo/pull/554))
- drop the lowest-value tenth of the suite ([#553](https://github.com/y-l-g/tablo/pull/553))
- seed the admin in one call and fix stale docs and module layout ([#541](https://github.com/y-l-g/tablo/pull/541))

## [0.4.0](https://github.com/y-l-g/tablo/compare/tablo-ui-v0.3.0...tablo-ui-v0.4.0) - 2026-10-07

### Other

- *(repo)* keep the rules in two short files and the ADRs to their decisions ([#525](https://github.com/y-l-g/tablo/pull/525))
- *(core)* [**breaking**] keep Topcoat's runtime as the only browser layer ([#516](https://github.com/y-l-g/tablo/pull/516))

## [0.3.0](https://github.com/y-l-g/tablo/compare/tablo-ui-v0.2.0...tablo-ui-v0.3.0) - 2026-10-06

### Added

- *(core)* confirm custom actions through a dialog ([#497](https://github.com/y-l-g/tablo/pull/497))

### Other

- *(repo)* add a README to every published crate ([#503](https://github.com/y-l-g/tablo/pull/503))

## [0.2.0](https://github.com/y-l-g/tablo/compare/tablo-ui-v0.1.0...tablo-ui-v0.2.0) - 2026-10-06

### Fixed

- *(repo)* inherit the repository URL in every crate ([#484](https://github.com/y-l-g/tablo/pull/484))
