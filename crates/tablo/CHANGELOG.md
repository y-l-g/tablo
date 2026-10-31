# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.0](https://github.com/y-l-g/tablo/compare/v0.8.0...v0.9.0) - 2026-10-31

### Added

- *(table)* [**breaking**] add a link to each row's actions with Table::row_link ([#908](https://github.com/y-l-g/tablo/pull/908))
- *(column)* set a text column's font weight with TextColumn::weight ([#905](https://github.com/y-l-g/tablo/pull/905))
- *(resource)* serve a resource's record pages under the record's URL ([#872](https://github.com/y-l-g/tablo/pull/872))
- *(panel)* read a list's state from its widgets with list_query ([#860](https://github.com/y-l-g/tablo/pull/860))
- *(table)* [**breaking**] offer a list's views as tabs with count badges ([#859](https://github.com/y-l-g/tablo/pull/859))
- *(table)* [**breaking**] edit a field in its cell with toggle, select and text columns ([#857](https://github.com/y-l-g/tablo/pull/857))
- *(table)* [**breaking**] link, copy, tooltip, limit, icon and tint a column's cells ([#856](https://github.com/y-l-g/tablo/pull/856))
- *(resource)* write an action's records in one statement with filter_keys ([#849](https://github.com/y-l-g/tablo/pull/849))
- *(values)* [**breaking**] bind rust_decimal and bigdecimal decimals ([#838](https://github.com/y-l-g/tablo/pull/838))
- [**breaking**] seal error and option enums as non-exhaustive ([#837](https://github.com/y-l-g/tablo/pull/837))
- *(widget)* list a resource's records in a widget's table ([#831](https://github.com/y-l-g/tablo/pull/831))
- *(chart)* draw line and bar charts on a widget ([#830](https://github.com/y-l-g/tablo/pull/830))
- *(panel)* add page widgets and stat tiles ([#820](https://github.com/y-l-g/tablo/pull/820))
- *(table)* apply the filter menu on an Apply button with Table::deferred_filters ([#909](https://github.com/y-l-g/tablo/pull/909))
- *(panel)* focus the global search box with Cmd+K or Ctrl+K ([#907](https://github.com/y-l-g/tablo/pull/907))
- *(macros)* require an Option field with #[form(required)] ([#906](https://github.com/y-l-g/tablo/pull/906))
- *(panel)* [**breaking**] show a dark-mode logo with Brand::dark_logo ([#904](https://github.com/y-l-g/tablo/pull/904))
- *(resource)* [**breaking**] label a public link with a Text ([#898](https://github.com/y-l-g/tablo/pull/898))
- *(relation)* [**breaking**] name a relation's foreign key with a lens ([#896](https://github.com/y-l-g/tablo/pull/896))
- *(write)* [**breaking**] decline a write with a message that names no field ([#893](https://github.com/y-l-g/tablo/pull/893))
- *(panel)* mark panel pages noindex through topcoat metadata ([#875](https://github.com/y-l-g/tablo/pull/875))
- *(resource)* split a record's pages into tabs ([#873](https://github.com/y-l-g/tablo/pull/873))
- *(schema)* [**breaking**] take typed values in a tags field ([#870](https://github.com/y-l-g/tablo/pull/870))
- *(column)* show a list column's values with ListColumn ([#869](https://github.com/y-l-g/tablo/pull/869))
- *(schema)* store a multiple choice in a list column ([#868](https://github.com/y-l-g/tablo/pull/868))
- *(schema)* [**breaking**] create a related record from a creatable choice ([#867](https://github.com/y-l-g/tablo/pull/867))
- *(resource)* [**breaking**] render a has_one relation as the related resource's form ([#864](https://github.com/y-l-g/tablo/pull/864))
- *(schema)* add disabled_when, hidden_on_create and hidden_on_edit ([#863](https://github.com/y-l-g/tablo/pull/863))
- *(table)* group a list across pages by one of several fields ([#861](https://github.com/y-l-g/tablo/pull/861))
- *(schema)* reorder a file list's rows ([#847](https://github.com/y-l-g/tablo/pull/847))
- *(schema)* [**breaking**] hold several files in one field with a file list ([#846](https://github.com/y-l-g/tablo/pull/846))
- *(resource)* [**breaking**] show an action disabled with the reason its state refuses ([#852](https://github.com/y-l-g/tablo/pull/852))
- *(resource)* customize an action's button: icon, tint, confirmation ([#851](https://github.com/y-l-g/tablo/pull/851))
- *(panel)* [**breaking**] run a bulk action on every record the list matches ([#850](https://github.com/y-l-g/tablo/pull/850))
- *(panel)* [**breaking**] give form pages section chrome, create another and a save shortcut ([#841](https://github.com/y-l-g/tablo/pull/841))
- *(macros)* [**breaking**] shape a derived column with #[columns(..)] keys ([#839](https://github.com/y-l-g/tablo/pull/839))
- *(resource)* hide a resource from the sidebar ([#840](https://github.com/y-l-g/tablo/pull/840))
- *(resource)* [**breaking**] type the seams of actions and record tables ([#836](https://github.com/y-l-g/tablo/pull/836))
- *(page)* render a page's declared header and widgets by default ([#835](https://github.com/y-l-g/tablo/pull/835))
- *(panel)* [**breaking**] let a panel set its export, import, bulk and body limits ([#834](https://github.com/y-l-g/tablo/pull/834))
- *(resource)* place widgets above a resource's list ([#833](https://github.com/y-l-g/tablo/pull/833))
- *(panel)* let a visitor pick which of a panel's locales they read it in ([#829](https://github.com/y-l-g/tablo/pull/829))
- *(tenancy)* [**breaking**] require every resource of a tenant panel to declare its tenancy ([#826](https://github.com/y-l-g/tablo/pull/826))
- *(auth)* [**breaking**] pass the panel's prefix to can_access_panel ([#823](https://github.com/y-l-g/tablo/pull/823))

### Changed

- *(resource)* [**breaking**] switch record pages and panel features off one way ([#889](https://github.com/y-l-g/tablo/pull/889))
- *(deps)* [**breaking**] track Topcoat main and adopt its unified context ([#822](https://github.com/y-l-g/tablo/pull/822))
- [**breaking**] drop Includes::len and redundant Sized bounds ([#900](https://github.com/y-l-g/tablo/pull/900))
- *(table)* [**breaking**] set a table's page size with page_size ([#897](https://github.com/y-l-g/tablo/pull/897))
- *(action)* [**breaking**] key an action's refusal by its input's field enum ([#894](https://github.com/y-l-g/tablo/pull/894))
- *(extend)* [**breaking**] seal the framework's own Column and Filter methods ([#888](https://github.com/y-l-g/tablo/pull/888))
- *(policy)* [**breaking**] ask View to open a record, never to read rows ([#885](https://github.com/y-l-g/tablo/pull/885))
- *(resource)* [**breaking**] scope a resource's rows per panel with ResourceDef::query ([#884](https://github.com/y-l-g/tablo/pull/884))
- *(tenancy)* [**breaking**] carry the request tenant as a TenantId ([#817](https://github.com/y-l-g/tablo/pull/817))

### Fixed

- *(auth)* read a type no panel loads as nobody in every build ([#892](https://github.com/y-l-g/tablo/pull/892))
- *(write)* fail closed when a record fn writes outside the tenant ([#887](https://github.com/y-l-g/tablo/pull/887))
- *(table)* seed list signals from a navigation's URL ([#882](https://github.com/y-l-g/tablo/pull/882)) ([#883](https://github.com/y-l-g/tablo/pull/883))
- *(core)* link stat tiles through the runtime ([#881](https://github.com/y-l-g/tablo/pull/881))
- *(ui)* stop tab strips from scrolling vertically ([#880](https://github.com/y-l-g/tablo/pull/880))
- *(list)* keep a cell switch's thumb visible ([#879](https://github.com/y-l-g/tablo/pull/879))
- *(chart)* keep the data table from widening the page ([#878](https://github.com/y-l-g/tablo/pull/878))
- *(table)* keep sr-only labels from widening the page at 375px ([#865](https://github.com/y-l-g/tablo/pull/865))
- *(values)* [**breaking**] keep decimal columns unsortable and cap decimal input length ([#844](https://github.com/y-l-g/tablo/pull/844))
- [**breaking**] address the post-merge review of #835, #836, #839 and #840 ([#843](https://github.com/y-l-g/tablo/pull/843))
- *(panel)* keep the relation seed on a create-another landing ([#842](https://github.com/y-l-g/tablo/pull/842))
- *(tenancy)* act for tenants only on a panel that serves them ([#825](https://github.com/y-l-g/tablo/pull/825))
- *(repo)* exempt the list header Create link's DOM id from the catalog guard ([#818](https://github.com/y-l-g/tablo/pull/818))
- *(panel)* keep the list header Create link across page changes ([#816](https://github.com/y-l-g/tablo/pull/816))
- *(deps)* update tiptap to v3.32.1 with its rebuilt bundle ([#866](https://github.com/y-l-g/tablo/pull/866))

### Other

- Follow-ups from a review of #885–#896 ([#902](https://github.com/y-l-g/tablo/pull/902))

## [0.8.0](https://github.com/y-l-g/tablo/compare/v0.7.0...v0.8.0) - 2026-10-25

### Added

- *(core)* [**breaking**] translate panels with Fluent catalogs in the request's locale ([#772](https://github.com/y-l-g/tablo/pull/772))
- *(schema)* [**breaking**] add Tabs blocks to forms and detail pages ([#770](https://github.com/y-l-g/tablo/pull/770))
- *(panel)* add global search ([#765](https://github.com/y-l-g/tablo/pull/765))
- *(core)* [**breaking**] let record fns and action runs refuse with inline field errors ([#755](https://github.com/y-l-g/tablo/pull/755))
- *(core)* declare a policy one ability at a time, denying the rest ([#751](https://github.com/y-l-g/tablo/pull/751))
- *(schema)* [**breaking**] add a rich text field and column ([#750](https://github.com/y-l-g/tablo/pull/750))
- *(core)* [**breaking**] add a before_commit hook that runs inside every write's transaction ([#747](https://github.com/y-l-g/tablo/pull/747))
- *(core)* [**breaking**] restrict who views and edits a field of a resource ([#745](https://github.com/y-l-g/tablo/pull/745))
- *(core)* [**breaking**] let an action's run return an Outcome ([#723](https://github.com/y-l-g/tablo/pull/723))
- *(table)* declare a default sort with a direction ([#672](https://github.com/y-l-g/tablo/pull/672))
- *(build)* forward Toasty's `turso` driver feature ([#671](https://github.com/y-l-g/tablo/pull/671))
- *(table)* offer a row's actions in a menu ([#649](https://github.com/y-l-g/tablo/pull/649))
- *(table)* [**breaking**] page through tables by number, with a count and a page size ([#645](https://github.com/y-l-g/tablo/pull/645))
- *(table)* present values as badges, money, numbers, colors and images ([#644](https://github.com/y-l-g/tablo/pull/644))
- *(ui)* [**breaking**] restyle the panel and move a table's filters into a menu ([#768](https://github.com/y-l-g/tablo/pull/768))
- *(resource)* ask a change whether it is a run of one header action ([#799](https://github.com/y-l-g/tablo/pull/799))
- *(auth)* limit sign-ins per login across addresses ([#789](https://github.com/y-l-g/tablo/pull/789))
- *(auth)* [**breaking**] end the two-factor challenge and back off after wrong codes ([#788](https://github.com/y-l-g/tablo/pull/788))
- *(auth)* [**breaking**] make a sign-up page choose whether new accounts enter the panel ([#787](https://github.com/y-l-g/tablo/pull/787))
- *(auth)* end an admin's sessions when a panel write changes their credentials ([#786](https://github.com/y-l-g/tablo/pull/786))
- *(upload)* [**breaking**] delete the uploads of submissions that never saved ([#785](https://github.com/y-l-g/tablo/pull/785))
- *(upload)* delete the files a committed write lets go ([#784](https://github.com/y-l-g/tablo/pull/784))
- *(upload)* [**breaking**] serve `uploads_dir` files only at signed URLs that expire ([#783](https://github.com/y-l-g/tablo/pull/783))
- *(panel)* [**breaking**] send a CSP and hardening headers on every panel response ([#782](https://github.com/y-l-g/tablo/pull/782))
- *(core)* [**breaking**] translate option labels ([#781](https://github.com/y-l-g/tablo/pull/781))
- *(resource)* ask a change whether it ran an action ([#769](https://github.com/y-l-g/tablo/pull/769))
- *(schema)* refuse a file by its extension or size before storing it ([#764](https://github.com/y-l-g/tablo/pull/764))
- *(schema)* compare a text field with another field of its form ([#763](https://github.com/y-l-g/tablo/pull/763))
- *(schema)* add url, accepted, and min/max items rules ([#761](https://github.com/y-l-g/tablo/pull/761))
- *(schema)* add radio, toggle-button, switch, url, tel and affixed text controls ([#760](https://github.com/y-l-g/tablo/pull/760))
- *(core)* add Notifications::send_in and refuse db(cx) inside a write's transaction ([#756](https://github.com/y-l-g/tablo/pull/756))
- *(test)* send typed requests to a resource through TestClient::resource ([#754](https://github.com/y-l-g/tablo/pull/754))
- *(auth)* panic in debug builds when auth::user names a type no panel loads ([#753](https://github.com/y-l-g/tablo/pull/753))
- *(core)* import a CSV file through the create form ([#749](https://github.com/y-l-g/tablo/pull/749))
- *(core)* [**breaking**] soft-delete a resource declared with a deleted-at column ([#744](https://github.com/y-l-g/tablo/pull/744))
- *(panel)* [**breaking**] refuse an edit another save overtook on a #[version] model ([#739](https://github.com/y-l-g/tablo/pull/739))
- *(core)* [**breaking**] refuse a resource that declares no tenancy over tenant-owned rows ([#507](https://github.com/y-l-g/tablo/pull/507)) ([#738](https://github.com/y-l-g/tablo/pull/738))
- *(panel)* [**breaking**] store notifications and show them in a live bell ([#736](https://github.com/y-l-g/tablo/pull/736))
- *(url)* spell a record's routes from its resource ([#726](https://github.com/y-l-g/tablo/pull/726))
- *(auth)* register Tablo's own models with tablo::models! ([#725](https://github.com/y-l-g/tablo/pull/725))
- *(table)* [**breaking**] name a table's default sort by its field ([#724](https://github.com/y-l-g/tablo/pull/724))
- *(panel)* [**breaking**] answer a request outside every prefix as no panel ([#722](https://github.com/y-l-g/tablo/pull/722))
- *(auth)* default tenant and a password reveal toggle ([#717](https://github.com/y-l-g/tablo/pull/717))
- *(auth)* two-factor codes by email ([#716](https://github.com/y-l-g/tablo/pull/716))
- *(auth)* two-factor sign-in with an authenticator app ([#715](https://github.com/y-l-g/tablo/pull/715))
- *(auth)* tenant settings page ([#714](https://github.com/y-l-g/tablo/pull/714))
- *(auth)* tenant registration ([#713](https://github.com/y-l-g/tablo/pull/713))
- *(auth)* confirm an email change by mail ([#712](https://github.com/y-l-g/tablo/pull/712))
- *(auth)* [**breaking**] email verification ([#711](https://github.com/y-l-g/tablo/pull/711))
- *(panel)* [**breaking**] let a panel declare the shards and procedures its pages use ([#684](https://github.com/y-l-g/tablo/pull/684))
- *(auth)* [**breaking**] add a password reset that mails a single-use link ([#677](https://github.com/y-l-g/tablo/pull/677))
- *(table)* let each user toggle a list's columns ([#675](https://github.com/y-l-g/tablo/pull/675))
- *(auth)* [**breaking**] add a profile page for the signed-in user ([#676](https://github.com/y-l-g/tablo/pull/676))
- *(schema)* declare a text field's length, bounds and pattern ([#674](https://github.com/y-l-g/tablo/pull/674))
- *(panel)* [**breaking**] add a favicon to `Brand` and badges to sidebar entries ([#673](https://github.com/y-l-g/tablo/pull/673))
- *(panel)* render relation tables on the edit page ([#668](https://github.com/y-l-g/tablo/pull/668))
- *(panel)* show a failed page inside the shell ([#651](https://github.com/y-l-g/tablo/pull/651))
- *(panel)* put the user in a sidebar menu and trail every page ([#650](https://github.com/y-l-g/tablo/pull/650))
- *(table)* [**breaking**] lay tables out by their content and open a record from its row ([#648](https://github.com/y-l-g/tablo/pull/648))
- *(auth)* put the brand above the sign-in and sign-up cards ([#646](https://github.com/y-l-g/tablo/pull/646))

### Changed

- *(deps)* [**breaking**] update topcoat to 0.11 and drop its main patch ([#815](https://github.com/y-l-g/tablo/pull/815))
- *(core)* [**breaking**] merge `ActionInput` and `RepeaterItem` into `FormInput` ([#667](https://github.com/y-l-g/tablo/pull/667))
- *(i18n)* [**breaking**] read locales through topcoat::l10n ([#813](https://github.com/y-l-g/tablo/pull/813))
- *(auth)* [**breaking**] refuse a seam over another user type at compile time ([#801](https://github.com/y-l-g/tablo/pull/801))
- *(tenancy)* [**breaking**] type a membership's tenant as a TenantId ([#798](https://github.com/y-l-g/tablo/pull/798))
- *(auth)* [**breaking**] break the module cycles around auth ([#767](https://github.com/y-l-g/tablo/pull/767))
- *(core)* [**breaking**] give every public item one path ([#758](https://github.com/y-l-g/tablo/pull/758))
- *(core)* compile model-independent views once, in tablo-core ([#727](https://github.com/y-l-g/tablo/pull/727))
- *(auth)* [**breaking**] one AuthToken table and one AuthMail for mailed links ([#710](https://github.com/y-l-g/tablo/pull/710))
- *(core)* [**breaking**] name the error and empty-table hooks once, shared with tablo-test ([#700](https://github.com/y-l-g/tablo/pull/700))
- *(core)* let auth, resources and pages own their mount checks and routes ([#699](https://github.com/y-l-g/tablo/pull/699))
- *(core)* order the serving layer, and name its two remaining back-edges ([#698](https://github.com/y-l-g/tablo/pull/698))
- *(core)* order the modules inside each layer, and split form values out of `form` ([#697](https://github.com/y-l-g/tablo/pull/697))
- *(core)* lift columns out of `table` so table and detail stop naming each other ([#696](https://github.com/y-l-g/tablo/pull/696))
- *(table)* give each built-in column and filter its own file ([#695](https://github.com/y-l-g/tablo/pull/695))
- *(schema)* [**breaking**] bind lenses when they are declared, not at mount ([#664](https://github.com/y-l-g/tablo/pull/664))
- *(table)* [**breaking**] move the panel's URL scheme and list page UI out of `table` ([#658](https://github.com/y-l-g/tablo/pull/658))
- *(ui)* [**breaking**] publish only the primitives a custom page builds from ([#666](https://github.com/y-l-g/tablo/pull/666))
- *(deps)* [**breaking**] track Topcoat main until its next release ([#647](https://github.com/y-l-g/tablo/pull/647))

### Fixed

- *(deps)* update rust crate fluent-langneg to 0.14 ([#804](https://github.com/y-l-g/tablo/pull/804))
- *(db)* [**breaking**] refuse a second connection inside a write's transaction in every build ([#802](https://github.com/y-l-g/tablo/pull/802))
- *(auth)* run every serializable auth transaction again when a concurrent one aborts it ([#797](https://github.com/y-l-g/tablo/pull/797))
- *(auth)* retry a mailed link's token write when a concurrent one aborts it ([#796](https://github.com/y-l-g/tablo/pull/796))
- *(panel)* link the search results page only for a mounted panel's prefix ([#790](https://github.com/y-l-g/tablo/pull/790))
- *(schema)* close the validation gaps a review of the new rules found ([#766](https://github.com/y-l-g/tablo/pull/766))
- *(table)* [**breaking**] refuse relationship filter keys the user cannot view ([#618](https://github.com/y-l-g/tablo/pull/618)) ([#743](https://github.com/y-l-g/tablo/pull/743))
- *(core)* [**breaking**] read and show timestamps in the user's time zone ([#742](https://github.com/y-l-g/tablo/pull/742))
- *(table)* [**breaking**] show only the related records a relation column's resource shows ([#741](https://github.com/y-l-g/tablo/pull/741))
- *(panel)* render a unique value a concurrent write took inline ([#194](https://github.com/y-l-g/tablo/pull/194)) ([#740](https://github.com/y-l-g/tablo/pull/740))
- *(deps)* move the RustCrypto crates to digest 0.11 together ([#734](https://github.com/y-l-g/tablo/pull/734))
- *(deps)* update rust crate pluralizer to 0.6 ([#731](https://github.com/y-l-g/tablo/pull/731))
- [**breaking**] probe scoped composite indexes, keep a closed select closed, keep profile seconds, whole acronym slugs ([#694](https://github.com/y-l-g/tablo/pull/694))
- *(auth)* bound Argon2 runs, gate a declared shard's twin path, and fix the email upgrade note ([#693](https://github.com/y-l-g/tablo/pull/693))
- handle page 0, a toggle made a choice, blank option values and encoded breadcrumbs ([#691](https://github.com/y-l-g/tablo/pull/691))
- *(auth)* [**breaking**] answer a session-rotation outage with the outage page, match emails in any case ([#690](https://github.com/y-l-g/tablo/pull/690))
- *(resource)* [**breaking**] pluralize default names with the pluralizer crate ([#689](https://github.com/y-l-g/tablo/pull/689))
- *(ui)* mask a multiline password, cap a repeater's add button, and list every option of a searchable select ([#688](https://github.com/y-l-g/tablo/pull/688))
- *(table)* let the list's sort outrank the resource query's ordering, and export negative numbers as numbers ([#687](https://github.com/y-l-g/tablo/pull/687))
- *(auth)* revoke sessions on every panel sharing the user's credentials, and keep refused throttle counts ([#686](https://github.com/y-l-g/tablo/pull/686))
- *(panel)* probe a composite unique index on all its fields ([#682](https://github.com/y-l-g/tablo/pull/682))
- *(auth)* [**breaking**] hash and verify passwords off the async executor ([#683](https://github.com/y-l-g/tablo/pull/683))
- *(schema)* keep a timestamp's seconds when a save leaves it untouched ([#680](https://github.com/y-l-g/tablo/pull/680))
- *(schema)* accept a single choice's stored key past the option cap ([#678](https://github.com/y-l-g/tablo/pull/678))
- *(panel)* send a relation's bulk actions to its record ([#669](https://github.com/y-l-g/tablo/pull/669))
- *(panel)* prefer the sidebar entry whose query the request holds ([#661](https://github.com/y-l-g/tablo/pull/661))
- *(table)* export a date column's stored value ([#660](https://github.com/y-l-g/tablo/pull/660))
- *(panel)* [**breaking**] check a create's columns whatever the policy, and add `edit_only` ([#657](https://github.com/y-l-g/tablo/pull/657))
- refuse off-site image URLs, log a failed page's cause, and smaller review fixes ([#655](https://github.com/y-l-g/tablo/pull/655))
- *(panel)* offer the theme without a session, and trail only resources' pages ([#654](https://github.com/y-l-g/tablo/pull/654))
- *(table)* keep offset pages stable, rows' links clickable and ids per table ([#653](https://github.com/y-l-g/tablo/pull/653))
- *(panel)* centre the brand and hide the scrollbar in the icon rail ([#652](https://github.com/y-l-g/tablo/pull/652))
- *(panel)* [**breaking**] keep the theme in one Tablo-named cookie ([#642](https://github.com/y-l-g/tablo/pull/642))
- *(ui)* close a select's list on Escape without closing its dialog ([#681](https://github.com/y-l-g/tablo/pull/681))
- *(ui)* keep a searchable select's server matches in its list ([#679](https://github.com/y-l-g/tablo/pull/679))

## [0.7.0](https://github.com/y-l-g/tablo/compare/v0.6.0...v0.7.0) - 2026-10-14

### Added

- *(table)* spell a date column as a calendar day ([#628](https://github.com/y-l-g/tablo/pull/628)) ([#629](https://github.com/y-l-g/tablo/pull/629))
- *(core)* [**breaking**] carry form values as typed FormValues ([#614](https://github.com/y-l-g/tablo/pull/614))
- *(core)* [**breaking**] declare a page as a PageDef and infer closure policies with policy_fn ([#608](https://github.com/y-l-g/tablo/pull/608))
- *(core)* [**breaking**] derive the default table and detail page from the model ([#607](https://github.com/y-l-g/tablo/pull/607))
- *(macros)* derive a model's record form with ModelForm ([#605](https://github.com/y-l-g/tablo/pull/605))
- *(build)* [**breaking**] ship Tablo's default theme in tablo-build ([#594](https://github.com/y-l-g/tablo/pull/594))
- *(table)* multi-select, date-range and relationship filters ([#617](https://github.com/y-l-g/tablo/pull/617))
- *(table)* [**breaking**] carry filter values as typed FilterValue
- *(schema)* add help text, disabled controls and create defaults to fields ([#597](https://github.com/y-l-g/tablo/pull/597))
- *(schema)* bind civil dates and times, and render numbers as number inputs ([#596](https://github.com/y-l-g/tablo/pull/596))
- *(core)* ship a directory uploader and Panel::uploads_dir ([#595](https://github.com/y-l-g/tablo/pull/595))
- *(core)* [**breaking**] group sidebar entries under labels ([#590](https://github.com/y-l-g/tablo/pull/590))

### Changed

- *(core)* dedup column sort order ([#638](https://github.com/y-l-g/tablo/pull/638))
- *(table)* share column plumbing and derive Clone/Debug with derive-where ([#593](https://github.com/y-l-g/tablo/pull/593))
- *(core)* [**breaking**] rename ResourceDef::view to detail ([#592](https://github.com/y-l-g/tablo/pull/592))
- *(core)* [**breaking**] build a panel handle, not a context, from Panel ([#591](https://github.com/y-l-g/tablo/pull/591))
- *(macros)* dedup struct-shape guards ([#640](https://github.com/y-l-g/tablo/pull/640))
- *(test)* dedup TestClient helpers ([#635](https://github.com/y-l-g/tablo/pull/635))

### Fixed

- *(table)* size the row actions column from its controls ([#626](https://github.com/y-l-g/tablo/pull/626))
- *(table)* draw a false boolean as a circled cross ([#627](https://github.com/y-l-g/tablo/pull/627))
- *(panel)* send the auth and retry links through the runtime ([#625](https://github.com/y-l-g/tablo/pull/625))
- *(table)* [**breaking**] follow up the filter controls ([#617](https://github.com/y-l-g/tablo/pull/617)) ([#622](https://github.com/y-l-g/tablo/pull/622))
- *(schema)* follow up the form values ([#614](https://github.com/y-l-g/tablo/pull/614)) ([#623](https://github.com/y-l-g/tablo/pull/623))
- *(table)* [**breaking**] follow up the FilterValue codec ([#616](https://github.com/y-l-g/tablo/pull/616)) ([#621](https://github.com/y-l-g/tablo/pull/621))
- *(panel)* follow up the sidebar and served-dir findings ([#624](https://github.com/y-l-g/tablo/pull/624))
- *(table)* hand the select-all handler a token string ([#619](https://github.com/y-l-g/tablo/pull/619)) ([#620](https://github.com/y-l-g/tablo/pull/620))
- *(table)* keep a key holding the selection separator whole in bulk writes ([#619](https://github.com/y-l-g/tablo/pull/619))
- *(panel)* [**breaking**] route the list's own pages under `-` so record keys cannot shadow them ([#610](https://github.com/y-l-g/tablo/pull/610))
- *(core)* fail closed on an input refusal no control renders ([#588](https://github.com/y-l-g/tablo/pull/588))

### Test

- [**breaking**] promote helpers into tablo-test, pin refusals with trybuild, assert kinds over wording ([#598](https://github.com/y-l-g/tablo/pull/598))

## [0.6.0](https://github.com/y-l-g/tablo/compare/v0.5.1...v0.6.0) - 2026-10-10

### Added

- *(schema)* edit a #[document] list in a repeater, a row per item ([#573](https://github.com/y-l-g/tablo/pull/573))
- *(panel)* [**breaking**] run actions from record pages, list headers and custom pages ([#567](https://github.com/y-l-g/tablo/pull/567)) ([#568](https://github.com/y-l-g/tablo/pull/568))
- *(core)* [**breaking**] let a custom action ask for typed input before it runs ([#540](https://github.com/y-l-g/tablo/pull/540))
- *(core)* add `RelationColumn` and `CountColumn` over a `relation!` lens ([#539](https://github.com/y-l-g/tablo/pull/539))
- *(core)* [**breaking**] declare the detail page as typed columns ([#537](https://github.com/y-l-g/tablo/pull/537))
- *(auth)* let visitors sign up through a Registrar ([#577](https://github.com/y-l-g/tablo/pull/577))
- *(schema)* link records through a join model, from a form and a related table ([#576](https://github.com/y-l-g/tablo/pull/576))
- *(panel)* ask for an action's input in a dialog over the page ([#572](https://github.com/y-l-g/tablo/pull/572))
- *(schema)* narrow a relationship choice to the rows of another field's value ([#571](https://github.com/y-l-g/tablo/pull/571))
- *(schema)* show a field or a layout block only while another field posts a value ([#570](https://github.com/y-l-g/tablo/pull/570))
- *(auth)* throttle sign-in attempts per login and address ([#563](https://github.com/y-l-g/tablo/pull/563)) ([#566](https://github.com/y-l-g/tablo/pull/566))
- *(resource)* [**breaking**] title a record by a column lens with record_title ([#565](https://github.com/y-l-g/tablo/pull/565))
- *(panel)* gate a page with Page::can_access and list only what the user may open ([#562](https://github.com/y-l-g/tablo/pull/562))
- *(core)* hand a background job the mounted panel through PanelHandle ([#557](https://github.com/y-l-g/tablo/pull/557))
- *(core)* match a policy on an action's type with Ability::is_action ([#556](https://github.com/y-l-g/tablo/pull/556))
- *(test)* sign a test client in with TestClient::sign_in ([#555](https://github.com/y-l-g/tablo/pull/555))
- *(core)* [**breaking**] declare a relationship once, on the record form ([#551](https://github.com/y-l-g/tablo/pull/551))
- *(core)* [**breaking**] type a resource's form by its record form ([#546](https://github.com/y-l-g/tablo/pull/546))
- *(panel)* export the list as CSV from the page header ([#547](https://github.com/y-l-g/tablo/pull/547))
- *(core)* default an action's label to its name in sentence case ([#543](https://github.com/y-l-g/tablo/pull/543))
- *(core)* default a resource's label to its model name in sentence case ([#542](https://github.com/y-l-g/tablo/pull/542))
- *(core)* [**breaking**] bind `Options` enums as typed form scalars ([#538](https://github.com/y-l-g/tablo/pull/538))
- *(core)* [**breaking**] authorize custom actions through `RunAny` and `Run` policy abilities ([#536](https://github.com/y-l-g/tablo/pull/536))

### Fixed

- *(table)* search case-insensitively on PostgreSQL ([#584](https://github.com/y-l-g/tablo/pull/584))
- *(macros)* label an EmbeddedForm field in sentence case ([#582](https://github.com/y-l-g/tablo/pull/582))
- *(core)* keep a resource's icon and order on a replaced sidebar entry ([#581](https://github.com/y-l-g/tablo/pull/581))
- *(schema)* disable the hidden variant groups' controls ([#569](https://github.com/y-l-g/tablo/pull/569))
- *(panel)* [**breaking**] refuse a file field when the panel installs no uploader ([#560](https://github.com/y-l-g/tablo/pull/560))
- *(core)* render a blank value as a dash ([#548](https://github.com/y-l-g/tablo/pull/548))

### Other

- compile every rustdoc example ([#554](https://github.com/y-l-g/tablo/pull/554))
- *(core)* [**breaking**] gather the extension traits in `tablo::extend` ([#544](https://github.com/y-l-g/tablo/pull/544))
- seed the admin in one call and fix stale docs and module layout ([#541](https://github.com/y-l-g/tablo/pull/541))
- drop the lowest-value tenth of the suite ([#553](https://github.com/y-l-g/tablo/pull/553))
- reorganize the suite by feature and pin each behavior once ([#552](https://github.com/y-l-g/tablo/pull/552))
- *(ui)* say why selects.js is a script, citing tokio-rs/topcoat#504 ([#574](https://github.com/y-l-g/tablo/pull/574))

## [0.5.1](https://github.com/y-l-g/tablo/compare/v0.5.0...v0.5.1) - 2026-10-07

### Other

- *(core)* point the compat modules at the current upstream gaps ([#534](https://github.com/y-l-g/tablo/pull/534))

## [0.5.0](https://github.com/y-l-g/tablo/compare/v0.4.0...v0.5.0) - 2026-10-07

### Other

- *(core)* [**breaking**] name a create column by its typed path ([#529](https://github.com/y-l-g/tablo/pull/529))
- *(core)* [**breaking**] run deletes and custom actions through one mutation pipeline ([#528](https://github.com/y-l-g/tablo/pull/528))
- *(core)* read Toasty's internals only through toasty_compat ([#526](https://github.com/y-l-g/tablo/pull/526))

## [0.4.0](https://github.com/y-l-g/tablo/compare/v0.3.0...v0.4.0) - 2026-10-07

### Fixed

- *(core)* [**breaking**] let a resource name its public link ([#523](https://github.com/y-l-g/tablo/pull/523))
- *(guide)* mount the related resource in the background-job example ([#518](https://github.com/y-l-g/tablo/pull/518))

### Other

- *(core)* [**breaking**] make the record form the one source of truth ([#513](https://github.com/y-l-g/tablo/pull/513))
- *(schema)* [**breaking**] bind embedded paths at mount, not in a thread-local scope ([#512](https://github.com/y-l-g/tablo/pull/512))
- *(core)* build panel pages outside the resource-generic handlers ([#521](https://github.com/y-l-g/tablo/pull/521))
- *(core)* correct binding claims and drop a dead accessor ([#520](https://github.com/y-l-g/tablo/pull/520))
- *(core)* render tables from a model-free frame ([#519](https://github.com/y-l-g/tablo/pull/519))
- *(core)* [**breaking**] build a context outside requests with Panel::context ([#517](https://github.com/y-l-g/tablo/pull/517))
- *(core)* [**breaking**] keep Topcoat's runtime as the only browser layer ([#516](https://github.com/y-l-g/tablo/pull/516))
- *(table)* [**breaking**] wire request affordances onto a WiredTable ([#511](https://github.com/y-l-g/tablo/pull/511))
- *(repo)* [**breaking**] home the test protocol helpers in tablo-test ([#509](https://github.com/y-l-g/tablo/pull/509))
- *(repo)* keep the rules in two short files and the ADRs to their decisions ([#525](https://github.com/y-l-g/tablo/pull/525))

## [0.3.0](https://github.com/y-l-g/tablo/compare/v0.2.0...v0.3.0) - 2026-10-06

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

## [0.2.0](https://github.com/y-l-g/tablo/compare/v0.1.0...v0.2.0) - 2026-10-06

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
