# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.5.1](https://github.com/y-l-g/tablo/compare/tablo-core-v0.5.0...tablo-core-v0.5.1) - 2026-10-07

### Other

- *(core)* point the compat modules at the current upstream gaps ([#534](https://github.com/y-l-g/tablo/pull/534))

## [0.5.0](https://github.com/y-l-g/tablo/compare/tablo-core-v0.4.0...tablo-core-v0.5.0) - 2026-10-07

### Other

- *(core)* [**breaking**] name a create column by its typed path ([#529](https://github.com/y-l-g/tablo/pull/529))
- *(core)* [**breaking**] run deletes and custom actions through one mutation pipeline ([#528](https://github.com/y-l-g/tablo/pull/528))
- *(core)* read Toasty's internals only through toasty_compat ([#526](https://github.com/y-l-g/tablo/pull/526))

## [0.4.0](https://github.com/y-l-g/tablo/compare/tablo-core-v0.3.0...tablo-core-v0.4.0) - 2026-10-07

### Fixed

- *(core)* [**breaking**] let a resource name its public link ([#523](https://github.com/y-l-g/tablo/pull/523))
- *(guide)* mount the related resource in the background-job example ([#518](https://github.com/y-l-g/tablo/pull/518))

### Other

- *(core)* build panel pages outside the resource-generic handlers ([#521](https://github.com/y-l-g/tablo/pull/521))
- *(core)* correct binding claims and drop a dead accessor ([#520](https://github.com/y-l-g/tablo/pull/520))
- *(core)* render tables from a model-free frame ([#519](https://github.com/y-l-g/tablo/pull/519))
- *(core)* [**breaking**] build a context outside requests with Panel::context ([#517](https://github.com/y-l-g/tablo/pull/517))
- *(core)* [**breaking**] keep Topcoat's runtime as the only browser layer ([#516](https://github.com/y-l-g/tablo/pull/516))
- *(core)* [**breaking**] make the record form the one source of truth ([#513](https://github.com/y-l-g/tablo/pull/513))
- *(schema)* [**breaking**] bind embedded paths at mount, not in a thread-local scope ([#512](https://github.com/y-l-g/tablo/pull/512))
- *(table)* [**breaking**] wire request affordances onto a WiredTable ([#511](https://github.com/y-l-g/tablo/pull/511))
- *(repo)* [**breaking**] home the test protocol helpers in tablo-test ([#509](https://github.com/y-l-g/tablo/pull/509))

## [0.3.0](https://github.com/y-l-g/tablo/compare/tablo-core-v0.2.0...tablo-core-v0.3.0) - 2026-10-06

### Added

- *(core)* [**breaking**] run a bulk action on the records it allows ([#426](https://github.com/y-l-g/tablo/pull/426)) ([#504](https://github.com/y-l-g/tablo/pull/504))
- *(core)* confirm custom actions through a dialog ([#497](https://github.com/y-l-g/tablo/pull/497))
- *(core)* [**breaking**] type Committed::acted by action ([#494](https://github.com/y-l-g/tablo/pull/494))
- *(core)* [**breaking**] seal public enums as non-exhaustive ([#498](https://github.com/y-l-g/tablo/pull/498))
- *(repo)* declare publish metadata and family versions ([#480](https://github.com/y-l-g/tablo/pull/480))
- *(core)* compose tables from column collections ([#472](https://github.com/y-l-g/tablo/pull/472))
- *(core)* [**breaking**] pass Cx to Action label and success ([#426](https://github.com/y-l-g/tablo/pull/426)) ([#469](https://github.com/y-l-g/tablo/pull/469))
- *(core)* export contains_expr, default can_run, add BooleanColumn width ([#426](https://github.com/y-l-g/tablo/pull/426)) ([#466](https://github.com/y-l-g/tablo/pull/466))
- *(core)* [**breaking**] report declaration mistakes as typed errors ([#460](https://github.com/y-l-g/tablo/pull/460))
- *(core)* [**breaking**] resolve declarations at mount and derive the table and view ([#453](https://github.com/y-l-g/tablo/pull/453))
- *(table)* [**breaking**] name fields once with lens! and read keys off the record (#385, #431)
- *(tablo-core)* [**breaking**] the app's own panel user type, with tenant memberships and a switcher ([#435](https://github.com/y-l-g/tablo/pull/435)) ([#436](https://github.com/y-l-g/tablo/pull/436))
- *(tablo-core)* [**breaking**] policy and tenancy declarations, relationship keys re-checked in the write ([#432](https://github.com/y-l-g/tablo/pull/432)) ([#433](https://github.com/y-l-g/tablo/pull/433))
- *(tablo-core)* [**breaking**] mount panels into an app-owned router, several per router ([#429](https://github.com/y-l-g/tablo/pull/429)) ([#430](https://github.com/y-l-g/tablo/pull/430))
- *(core)* [**breaking**] serve cached declarations, typed field builders, derived default forms ([#427](https://github.com/y-l-g/tablo/pull/427)) ([#428](https://github.com/y-l-g/tablo/pull/428))
- *(tablo-core)* [**breaking**] open extension traits for columns, filters, controls and actions ([#424](https://github.com/y-l-g/tablo/pull/424))
- *(repo)* [**breaking**] build a Tablo app outside this repository ([#421](https://github.com/y-l-g/tablo/pull/421))
- *(tablo-core)* live in-place relation tables ([#417](https://github.com/y-l-g/tablo/pull/417))
- *(ui)* [**breaking**] one consistent, token-styled panel UI ([#416](https://github.com/y-l-g/tablo/pull/416))
- *(schema)* [**breaking**] take the scalar blank rule for an embedded leaf ([#371](https://github.com/y-l-g/tablo/pull/371)) ([#411](https://github.com/y-l-g/tablo/pull/411))
- *(core)* [**breaking**] relation tables and create from the parent page ([#408](https://github.com/y-l-g/tablo/pull/408)) ([#413](https://github.com/y-l-g/tablo/pull/413))
- *(core)* register app pages and a home page on the panel ([#407](https://github.com/y-l-g/tablo/pull/407)) ([#410](https://github.com/y-l-g/tablo/pull/410))
- *(core)* navigate between panel pages without a document load ([#395](https://github.com/y-l-g/tablo/pull/395)) ([#396](https://github.com/y-l-g/tablo/pull/396))
- *(core)* [**breaking**] derive row chrome from the policy and the form ([#383](https://github.com/y-l-g/tablo/pull/383)) ([#391](https://github.com/y-l-g/tablo/pull/391))
- *(core)* [**breaking**] merge FormResource into Resource with one registration ([#382](https://github.com/y-l-g/tablo/pull/382)) ([#390](https://github.com/y-l-g/tablo/pull/390))
- *(table)* [**breaking**] require the key in the Table constructor and seal panel wiring ([#384](https://github.com/y-l-g/tablo/pull/384)) ([#388](https://github.com/y-l-g/tablo/pull/388))
- *(core)* [**breaking**] derive a typed record form and default the record fns ([#369](https://github.com/y-l-g/tablo/pull/369))
- *(table)* unify Table::id and Table::pk into Table::key ([#340](https://github.com/y-l-g/tablo/pull/340)) ([#357](https://github.com/y-l-g/tablo/pull/357))
- *(showcase)* [**breaking**] wordpress-style media library, computed counts, datetime-local ([#338](https://github.com/y-l-g/tablo/pull/338)) ([#348](https://github.com/y-l-g/tablo/pull/348))

### Fixed

- *(schema)* render Toggle with the shared checkbox ([#496](https://github.com/y-l-g/tablo/pull/496))
- *(table)* describe row actions by their row ([#495](https://github.com/y-l-g/tablo/pull/495))
- *(repo)* home the protocol helpers in tablo-core ([#493](https://github.com/y-l-g/tablo/pull/493))
- *(repo)* inherit the repository URL in every crate ([#484](https://github.com/y-l-g/tablo/pull/484))
- *(repo)* break the macros-core dev-dependency cycle ([#481](https://github.com/y-l-g/tablo/pull/481))
- *(core)* log declaration failures ([#471](https://github.com/y-l-g/tablo/pull/471))
- *(panel)* guard action routes with a dash segment ([#426](https://github.com/y-l-g/tablo/pull/426)) ([#467](https://github.com/y-l-g/tablo/pull/467))
- *(core)* order the base relationship option load ([#445](https://github.com/y-l-g/tablo/pull/445))
- *(auth)* sweep expired sessions on login (#302, #312, #389) ([#412](https://github.com/y-l-g/tablo/pull/412))
- *(core)* address the review of the simplification PR 2 ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#404](https://github.com/y-l-g/tablo/pull/404))
- *(core)* stop the CSV export walk on the cursor, not the page length ([#232](https://github.com/y-l-g/tablo/pull/232)) ([#380](https://github.com/y-l-g/tablo/pull/380))
- *(core)* lay out relation tables fixed with ColumnWidth widths ([#264](https://github.com/y-l-g/tablo/pull/264)) ([#374](https://github.com/y-l-g/tablo/pull/374))
- *(table)* hide rows the row policy refuses ([#353](https://github.com/y-l-g/tablo/pull/353)) ([#354](https://github.com/y-l-g/tablo/pull/354))
- *(schema)* unify titled-group containers and comment textarea ([#333](https://github.com/y-l-g/tablo/pull/333)) ([#335](https://github.com/y-l-g/tablo/pull/335))
- *(table)* locked-row badge and narrow-viewport min-width ([#334](https://github.com/y-l-g/tablo/pull/334)) ([#336](https://github.com/y-l-g/tablo/pull/336))

### Other

- *(repo)* add a README to every published crate ([#503](https://github.com/y-l-g/tablo/pull/503))
- release v0.2.0 ([#490](https://github.com/y-l-g/tablo/pull/490))
- *(suite)* delete duplicated tests and harden the test rules ([#473](https://github.com/y-l-g/tablo/pull/473))
- *(fixtures)* share one DummyUser, Subscriber, and nickname model ([#470](https://github.com/y-l-g/tablo/pull/470))
- *(suite)* assert rendered behavior through DOM queries ([#465](https://github.com/y-l-g/tablo/pull/465))
- *(harness)* add semantic HTML queries ([#462](https://github.com/y-l-g/tablo/pull/462))
- *(core)* [**breaking**] declare resources as values and layer tablo-core ([#464](https://github.com/y-l-g/tablo/pull/464))
- *(core)* add topcoat_compat for #123 and #399 ([#457](https://github.com/y-l-g/tablo/pull/457))
- *(repo)* compile the guide and rustdoc examples ([#454](https://github.com/y-l-g/tablo/pull/454))
- *(repo)* trim ADRs, vocabulary and contributor docs to single sources ([#455](https://github.com/y-l-g/tablo/pull/455))
- *(repo)* trim remaining comments to WHAT-only, compact ADRs ([#448](https://github.com/y-l-g/tablo/pull/448))
- *(repo)* trim comments to WHAT-only, compact ADRs ([#447](https://github.com/y-l-g/tablo/pull/447))
- *(core)* record fn errors keep their mapping ([#444](https://github.com/y-l-g/tablo/pull/444))
- *(repo)* strip issue history from comments, make issue refs optional ([#437](https://github.com/y-l-g/tablo/pull/437))
- *(repo)* rewrite the guide and glossary against the code ([#420](https://github.com/y-l-g/tablo/pull/420))
- *(core)* [**breaking**] one error vocabulary and the public page document ([#392](https://github.com/y-l-g/tablo/pull/392))
- *(core)* [**breaking**] the URL query is the one list state ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#405](https://github.com/y-l-g/tablo/pull/405))
- *(core)* [**breaking**] one field type and compiled schema ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#403](https://github.com/y-l-g/tablo/pull/403))
- *(core)* [**breaking**] typed column includes and one table loader ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#402](https://github.com/y-l-g/tablo/pull/402))
- *(core)* [**breaking**] fixes and dead code from the simplification spec ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#401](https://github.com/y-l-g/tablo/pull/401))
- move unit test modules into sibling tests.rs files ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#400](https://github.com/y-l-g/tablo/pull/400))
- *(core)* drain or drop streamed list bodies on single-connection pools ([#306](https://github.com/y-l-g/tablo/pull/306)) ([#381](https://github.com/y-l-g/tablo/pull/381))
- *(tablo-core)* split panel/actions and Panel build, deny too_many_lines ([#346](https://github.com/y-l-g/tablo/pull/346)) ([#362](https://github.com/y-l-g/tablo/pull/362))
- *(tablo-core)* split panel/forms and table/render into submodules ([#361](https://github.com/y-l-g/tablo/pull/361))
- *(harness)* extract shared helpers into tablo-test ([#343](https://github.com/y-l-g/tablo/pull/343)) ([#359](https://github.com/y-l-g/tablo/pull/359))
- *(assets)* dedupe selection-wire codec into shared wire.js ([#341](https://github.com/y-l-g/tablo/pull/341)) ([#356](https://github.com/y-l-g/tablo/pull/356))
- *(repo)* remove Phase 1 and Phase 2 labels ([#330](https://github.com/y-l-g/tablo/pull/330)) ([#332](https://github.com/y-l-g/tablo/pull/332))
- *(repo)* [**breaking**] rename argentum to tablo ([#326](https://github.com/y-l-g/tablo/pull/326))

## [0.2.0](https://github.com/y-l-g/tablo/compare/tablo-core-v0.1.0...tablo-core-v0.2.0) - 2026-10-06

### Added

- *(repo)* declare publish metadata and family versions ([#480](https://github.com/y-l-g/tablo/pull/480))
- *(core)* compose tables from column collections ([#472](https://github.com/y-l-g/tablo/pull/472))
- *(core)* [**breaking**] pass Cx to Action label and success ([#426](https://github.com/y-l-g/tablo/pull/426)) ([#469](https://github.com/y-l-g/tablo/pull/469))
- *(core)* export contains_expr, default can_run, add BooleanColumn width ([#426](https://github.com/y-l-g/tablo/pull/426)) ([#466](https://github.com/y-l-g/tablo/pull/466))
- *(core)* [**breaking**] report declaration mistakes as typed errors ([#460](https://github.com/y-l-g/tablo/pull/460))
- *(core)* [**breaking**] resolve declarations at mount and derive the table and view ([#453](https://github.com/y-l-g/tablo/pull/453))
- *(table)* [**breaking**] name fields once with lens! and read keys off the record (#385, #431)
- *(tablo-core)* [**breaking**] the app's own panel user type, with tenant memberships and a switcher ([#435](https://github.com/y-l-g/tablo/pull/435)) ([#436](https://github.com/y-l-g/tablo/pull/436))
- *(tablo-core)* [**breaking**] policy and tenancy declarations, relationship keys re-checked in the write ([#432](https://github.com/y-l-g/tablo/pull/432)) ([#433](https://github.com/y-l-g/tablo/pull/433))
- *(tablo-core)* [**breaking**] mount panels into an app-owned router, several per router ([#429](https://github.com/y-l-g/tablo/pull/429)) ([#430](https://github.com/y-l-g/tablo/pull/430))
- *(core)* [**breaking**] serve cached declarations, typed field builders, derived default forms ([#427](https://github.com/y-l-g/tablo/pull/427)) ([#428](https://github.com/y-l-g/tablo/pull/428))
- *(tablo-core)* [**breaking**] open extension traits for columns, filters, controls and actions ([#424](https://github.com/y-l-g/tablo/pull/424))
- *(repo)* [**breaking**] build a Tablo app outside this repository ([#421](https://github.com/y-l-g/tablo/pull/421))
- *(tablo-core)* live in-place relation tables ([#417](https://github.com/y-l-g/tablo/pull/417))
- *(ui)* [**breaking**] one consistent, token-styled panel UI ([#416](https://github.com/y-l-g/tablo/pull/416))
- *(schema)* [**breaking**] take the scalar blank rule for an embedded leaf ([#371](https://github.com/y-l-g/tablo/pull/371)) ([#411](https://github.com/y-l-g/tablo/pull/411))
- *(core)* [**breaking**] relation tables and create from the parent page ([#408](https://github.com/y-l-g/tablo/pull/408)) ([#413](https://github.com/y-l-g/tablo/pull/413))
- *(core)* register app pages and a home page on the panel ([#407](https://github.com/y-l-g/tablo/pull/407)) ([#410](https://github.com/y-l-g/tablo/pull/410))
- *(core)* navigate between panel pages without a document load ([#395](https://github.com/y-l-g/tablo/pull/395)) ([#396](https://github.com/y-l-g/tablo/pull/396))
- *(core)* [**breaking**] derive row chrome from the policy and the form ([#383](https://github.com/y-l-g/tablo/pull/383)) ([#391](https://github.com/y-l-g/tablo/pull/391))
- *(core)* [**breaking**] merge FormResource into Resource with one registration ([#382](https://github.com/y-l-g/tablo/pull/382)) ([#390](https://github.com/y-l-g/tablo/pull/390))
- *(table)* [**breaking**] require the key in the Table constructor and seal panel wiring ([#384](https://github.com/y-l-g/tablo/pull/384)) ([#388](https://github.com/y-l-g/tablo/pull/388))
- *(core)* [**breaking**] derive a typed record form and default the record fns ([#369](https://github.com/y-l-g/tablo/pull/369))
- *(table)* unify Table::id and Table::pk into Table::key ([#340](https://github.com/y-l-g/tablo/pull/340)) ([#357](https://github.com/y-l-g/tablo/pull/357))
- *(showcase)* [**breaking**] wordpress-style media library, computed counts, datetime-local ([#338](https://github.com/y-l-g/tablo/pull/338)) ([#348](https://github.com/y-l-g/tablo/pull/348))

### Fixed

- *(repo)* inherit the repository URL in every crate ([#484](https://github.com/y-l-g/tablo/pull/484))
- *(repo)* break the macros-core dev-dependency cycle ([#481](https://github.com/y-l-g/tablo/pull/481))
- *(core)* log declaration failures ([#471](https://github.com/y-l-g/tablo/pull/471))
- *(panel)* guard action routes with a dash segment ([#426](https://github.com/y-l-g/tablo/pull/426)) ([#467](https://github.com/y-l-g/tablo/pull/467))
- *(core)* order the base relationship option load ([#445](https://github.com/y-l-g/tablo/pull/445))
- *(auth)* sweep expired sessions on login (#302, #312, #389) ([#412](https://github.com/y-l-g/tablo/pull/412))
- *(core)* address the review of the simplification PR 2 ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#404](https://github.com/y-l-g/tablo/pull/404))
- *(core)* stop the CSV export walk on the cursor, not the page length ([#232](https://github.com/y-l-g/tablo/pull/232)) ([#380](https://github.com/y-l-g/tablo/pull/380))
- *(core)* lay out relation tables fixed with ColumnWidth widths ([#264](https://github.com/y-l-g/tablo/pull/264)) ([#374](https://github.com/y-l-g/tablo/pull/374))
- *(table)* hide rows the row policy refuses ([#353](https://github.com/y-l-g/tablo/pull/353)) ([#354](https://github.com/y-l-g/tablo/pull/354))
- *(schema)* unify titled-group containers and comment textarea ([#333](https://github.com/y-l-g/tablo/pull/333)) ([#335](https://github.com/y-l-g/tablo/pull/335))
- *(table)* locked-row badge and narrow-viewport min-width ([#334](https://github.com/y-l-g/tablo/pull/334)) ([#336](https://github.com/y-l-g/tablo/pull/336))

### Other

- *(suite)* delete duplicated tests and harden the test rules ([#473](https://github.com/y-l-g/tablo/pull/473))
- *(fixtures)* share one DummyUser, Subscriber, and nickname model ([#470](https://github.com/y-l-g/tablo/pull/470))
- *(suite)* assert rendered behavior through DOM queries ([#465](https://github.com/y-l-g/tablo/pull/465))
- *(harness)* add semantic HTML queries ([#462](https://github.com/y-l-g/tablo/pull/462))
- *(core)* [**breaking**] declare resources as values and layer tablo-core ([#464](https://github.com/y-l-g/tablo/pull/464))
- *(core)* add topcoat_compat for #123 and #399 ([#457](https://github.com/y-l-g/tablo/pull/457))
- *(repo)* compile the guide and rustdoc examples ([#454](https://github.com/y-l-g/tablo/pull/454))
- *(repo)* trim ADRs, vocabulary and contributor docs to single sources ([#455](https://github.com/y-l-g/tablo/pull/455))
- *(repo)* trim remaining comments to WHAT-only, compact ADRs ([#448](https://github.com/y-l-g/tablo/pull/448))
- *(repo)* trim comments to WHAT-only, compact ADRs ([#447](https://github.com/y-l-g/tablo/pull/447))
- *(core)* record fn errors keep their mapping ([#444](https://github.com/y-l-g/tablo/pull/444))
- *(repo)* strip issue history from comments, make issue refs optional ([#437](https://github.com/y-l-g/tablo/pull/437))
- *(repo)* rewrite the guide and glossary against the code ([#420](https://github.com/y-l-g/tablo/pull/420))
- *(core)* [**breaking**] one error vocabulary and the public page document ([#392](https://github.com/y-l-g/tablo/pull/392))
- *(core)* [**breaking**] the URL query is the one list state ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#405](https://github.com/y-l-g/tablo/pull/405))
- *(core)* [**breaking**] one field type and compiled schema ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#403](https://github.com/y-l-g/tablo/pull/403))
- *(core)* [**breaking**] typed column includes and one table loader ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#402](https://github.com/y-l-g/tablo/pull/402))
- *(core)* [**breaking**] fixes and dead code from the simplification spec ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#401](https://github.com/y-l-g/tablo/pull/401))
- move unit test modules into sibling tests.rs files ([#392](https://github.com/y-l-g/tablo/pull/392)) ([#400](https://github.com/y-l-g/tablo/pull/400))
- *(core)* drain or drop streamed list bodies on single-connection pools ([#306](https://github.com/y-l-g/tablo/pull/306)) ([#381](https://github.com/y-l-g/tablo/pull/381))
- *(tablo-core)* split panel/actions and Panel build, deny too_many_lines ([#346](https://github.com/y-l-g/tablo/pull/346)) ([#362](https://github.com/y-l-g/tablo/pull/362))
- *(tablo-core)* split panel/forms and table/render into submodules ([#361](https://github.com/y-l-g/tablo/pull/361))
- *(harness)* extract shared helpers into tablo-test ([#343](https://github.com/y-l-g/tablo/pull/343)) ([#359](https://github.com/y-l-g/tablo/pull/359))
- *(assets)* dedupe selection-wire codec into shared wire.js ([#341](https://github.com/y-l-g/tablo/pull/341)) ([#356](https://github.com/y-l-g/tablo/pull/356))
- *(repo)* remove Phase 1 and Phase 2 labels ([#330](https://github.com/y-l-g/tablo/pull/330)) ([#332](https://github.com/y-l-g/tablo/pull/332))
- *(repo)* [**breaking**] rename argentum to tablo ([#326](https://github.com/y-l-g/tablo/pull/326))
