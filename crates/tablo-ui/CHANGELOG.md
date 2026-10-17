# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.8.0](https://github.com/y-l-g/tablo/compare/tablo-ui-v0.7.0...tablo-ui-v0.8.0) - 2026-10-17

### Added

- *(table)* let each user toggle a list's columns ([#675](https://github.com/y-l-g/tablo/pull/675))
- *(auth)* [**breaking**] add a profile page for the signed-in user ([#676](https://github.com/y-l-g/tablo/pull/676))
- *(panel)* render relation tables on the edit page ([#668](https://github.com/y-l-g/tablo/pull/668))
- *(panel)* put the user in a sidebar menu and trail every page ([#650](https://github.com/y-l-g/tablo/pull/650))
- *(table)* offer a row's actions in a menu ([#649](https://github.com/y-l-g/tablo/pull/649))
- *(table)* present values as badges, money, numbers, colors and images ([#644](https://github.com/y-l-g/tablo/pull/644))

### Changed

- *(ui)* [**breaking**] publish only the primitives a custom page builds from ([#666](https://github.com/y-l-g/tablo/pull/666))
- *(deps)* [**breaking**] track Topcoat main until its next release ([#647](https://github.com/y-l-g/tablo/pull/647))

### Fixed

- [**breaking**] probe scoped composite indexes, keep a closed select closed, keep profile seconds, whole acronym slugs ([#694](https://github.com/y-l-g/tablo/pull/694))
- *(ui)* mask a multiline password, cap a repeater's add button, and list every option of a searchable select ([#688](https://github.com/y-l-g/tablo/pull/688))
- *(ui)* close a select's list on Escape without closing its dialog ([#681](https://github.com/y-l-g/tablo/pull/681))
- *(ui)* keep a searchable select's server matches in its list ([#679](https://github.com/y-l-g/tablo/pull/679))
- *(panel)* [**breaking**] keep the theme in one Tablo-named cookie ([#642](https://github.com/y-l-g/tablo/pull/642))

## [0.7.0](https://github.com/y-l-g/tablo/compare/tablo-ui-v0.6.0...tablo-ui-v0.7.0) - 2026-10-14

### Fixed

- *(table)* draw a false boolean as a circled cross ([#627](https://github.com/y-l-g/tablo/pull/627))
- *(panel)* [**breaking**] route the list's own pages under `-` so record keys cannot shadow them ([#610](https://github.com/y-l-g/tablo/pull/610))

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
