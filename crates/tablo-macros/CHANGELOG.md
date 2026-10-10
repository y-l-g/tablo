# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.6.0](https://github.com/y-l-g/tablo/compare/tablo-macros-v0.5.1...tablo-macros-v0.6.0) - 2026-10-10

### Added

- *(auth)* let visitors sign up through a Registrar ([#577](https://github.com/y-l-g/tablo/pull/577))
- *(schema)* link records through a join model, from a form and a related table ([#576](https://github.com/y-l-g/tablo/pull/576))
- *(schema)* edit a #[document] list in a repeater, a row per item ([#573](https://github.com/y-l-g/tablo/pull/573))
- *(core)* [**breaking**] declare a relationship once, on the record form ([#551](https://github.com/y-l-g/tablo/pull/551))
- *(core)* [**breaking**] type a resource's form by its record form ([#546](https://github.com/y-l-g/tablo/pull/546))
- *(core)* [**breaking**] let a custom action ask for typed input before it runs ([#540](https://github.com/y-l-g/tablo/pull/540))
- *(core)* [**breaking**] bind `Options` enums as typed form scalars ([#538](https://github.com/y-l-g/tablo/pull/538))
- *(core)* [**breaking**] declare the detail page as typed columns ([#537](https://github.com/y-l-g/tablo/pull/537))

### Fixed

- *(macros)* label an EmbeddedForm field in sentence case ([#582](https://github.com/y-l-g/tablo/pull/582))

### Other

- reorganize the suite by feature and pin each behavior once ([#552](https://github.com/y-l-g/tablo/pull/552))

## [0.4.0](https://github.com/y-l-g/tablo/compare/tablo-macros-v0.3.0...tablo-macros-v0.4.0) - 2026-10-07

### Other

- *(core)* [**breaking**] make the record form the one source of truth ([#513](https://github.com/y-l-g/tablo/pull/513))
- *(schema)* [**breaking**] bind embedded paths at mount, not in a thread-local scope ([#512](https://github.com/y-l-g/tablo/pull/512))

## [0.3.0](https://github.com/y-l-g/tablo/compare/tablo-macros-v0.2.0...tablo-macros-v0.3.0) - 2026-10-06

### Other

- *(repo)* add a README to every published crate ([#503](https://github.com/y-l-g/tablo/pull/503))

## [0.2.0](https://github.com/y-l-g/tablo/compare/tablo-macros-v0.1.0...tablo-macros-v0.2.0) - 2026-10-06

### Added

- *(repo)* declare publish metadata and family versions ([#480](https://github.com/y-l-g/tablo/pull/480))

### Fixed

- *(repo)* inherit the repository URL in every crate ([#484](https://github.com/y-l-g/tablo/pull/484))
- *(repo)* break the macros-core dev-dependency cycle ([#481](https://github.com/y-l-g/tablo/pull/481))
