# Tablo

Admin toolkit for Rust, server-rendered on Topcoat and persisted with Toasty. A **Panel** serves
one **Resource** per model; each resource declares a **Table** for its list and a **Schema** for
its forms and detail page, and writes through its record functions.

This file defines the project's vocabulary: use these words in code, comments, issues and commits,
and avoid the listed synonyms. The [user guide](docs/guide/src/introduction.md) explains how each
part behaves.

## Language

### Panel

An admin panel: its Resources and Pages under one prefix, its Shell, and its authentication gate.
The app owns the Router and the `Db` in its app context, and mounts the panel with
`RouterBuilderPanelExt::panel`, which turns the declarations into routes, navigation and the
layout that frames them in the Shell. One Router mounts several panels at distinct prefixes; each
request is served by the panel whose prefix it is under.

_Avoid_: Admin, Dashboard, App, Site

_Documented exceptions_: the `AdminUser` model keeps the `Admin` prefix, and `/admin` is the
conventional mount prefix, not Panel vocabulary.

### Page

A panel page that is not a Resource: a type implementing `tablo_core::Page`, registered with
`Panel::page` at `{prefix}/{slug}` or with `Panel::home` at the prefix itself. The Panel owns its
route and its NavigationItem; the page owns its markup.

_Avoid_: CustomPage, Screen, View

### Resource

A type implementing `Resource` that maps one Toasty model to its admin UI: its Query, Table,
Schema, Policy, NavigationItem and record functions. One model has one Resource, registered with
`Panel::resource` on each panel that serves it. A Resource whose `Form` is a Record form has create and edit pages; one
that names `NoForm` is list-only.

_Avoid_: Model, Entity, Collection, AdminModel, CRUD

### Query

A Resource's base query, `Resource::query(cx)`: its own row scoping, such as soft deletes. Every
loader starts from `scoped_query`, which is that query with the Tenancy filter applied.

_Avoid_: Scope, EloquentQuery, Builder (as a domain term)

### Policy

A Resource's authorization, `Resource::policy()`: a value implementing the `Policy` trait that
answers one Ability at a time. The default is `Deny`. `Allow`, `ReadOnly` and `when(predicate)`
combine with `and` and `or`, and a closure over the context and the Ability is one too. Handlers
ask it, and the row actions a Table renders follow it.

_Avoid_: Guard, Permission, Gate, Rule

### Ability

One thing a Policy is asked to allow: `ViewAny`, `View(record)`, `Create`, `Update(record)`,
`DeleteAny` and `Delete(record)`.

_Avoid_: Permission, Action (an Action is a custom mutation), Verb

### Tenancy

How a Resource's rows belong to a tenant, `Resource::tenancy()`: none, a tenant column of the
model's own (`Tenancy::column(lens)`), or a tenant reached through a relation
(`Tenancy::via(lens)`). A scoped Resource answers 403 to a request with no tenant, and every
loader filters on its lens.

_Avoid_: Tenant scope, Multi-tenancy mode

### Table

The declaration of a Resource's list view: its Columns, Filters, search, sort, grouping and page
size. Its row key is the model's primary key, declared in the constructor
(`Table::new(|u| u.id.to_string(), columns)`) and never a loop index. `Table::new_split` separates
the key that identifies a row in the page from the primary key the action URLs carry.

_Avoid_: Grid, Listing, DataTable

_Documented exception_: `Grid` is also a Schema layout block, `Grid::new(2)`. Only the layout block
uses the name; the rendered list is a Table everywhere, including comments and local variables.

### Column

One cell of a Table row: a `Column` renders it from the record, as a view and as text for the
export. `TextColumn::r#for` binds a `String` field through a lens and may be searchable and
sortable; `TextColumn::computed` renders any value and supports neither; `BooleanColumn` renders a
`bool` as an icon. An app implements `Column` for its own. A Column that reads a relation declares
it (`TextColumn::include`, `Column::includes`).

_Avoid_: Field (in a table), Cell, Attribute

### Filter

A predicate a Table adds to its query from a UI control, implementing `Filter`: `SelectFilter`,
`TernaryFilter`, `DateFilter`, `VariantFilter`, or an app's own. Active Filters combine with AND.

_Avoid_: Scope, Constraint, Where

### Schema

The layout of a form or a detail page: a tree of layout blocks (`Section`, `Group`, `Grid`,
`Repeater`), Fields and Embedded values. A form renders it as controls; a detail page renders the
same tree as stored values.

_Avoid_: Form, Infolist, Fieldset (as a top-level term), statePath

### Field

One input in a Schema, bound to a model column through a lens: `Field::text`, `Field::choice`,
`Field::file`, `Field::toggle`, or `Field::custom` over an app's `Control`. Each constructor
returns its control's builder (`TextField`, `ChoiceField`, `FileField`, `CustomField`), which
offers only that control's modifiers, so a modifier on the wrong control does not compile. Its
label, requiredness and uniqueness default from the column. A closed set of values is an
`Options` enum, shared by the form, the filter and the column.

_Avoid_: Input, Control, Widget (in a form), statePath

### Embedded value

A `toasty::Embed` struct or enum stored in its parent's row as flattened columns, bound by a form
as one value rather than column by column. `#[derive(EmbeddedForm)]` generates its Schema node and
its conversion to and from form values.

_Avoid_: Nested form, Sub-form, Composite field, Inline model

### Record form

The typed struct a Resource's form submission parses into: `#[derive(RecordForm)]`, with one
field per model column the form writes, named and typed like the model's field. The derive emits
one control per field (`controls(dx)`), chosen from the field, and the default schema arranging
them in order; the resource overrides `form(dx)` to arrange them into a layout instead. The Panel
hydrates edit forms and detail pages from it and writes it through Toasty's builders.

_Avoid_: Patch, Draft, Input, DTO, Form (alone: that is the Schema)

### Completion

On an edit, filling every declared key the submission did not post from the stored record, so the
Record form parses whole and an unposted field keeps its value. The keys the submission did post
are **named**, and the update assigns only named fields.

_Avoid_: Presence, Backfill (as the general term), Merge, Default

### Posted

What `Resource::update_record` receives: the parsed Record form, which it dereferences to, and the
set of fields the submission named.

_Avoid_: Patch, Changes, Diff, Submission

### Action

A user-invoked mutation, run by a POST handler that checks Policy inside a transaction: a create,
update or delete through the Resource's record function (`create_record`, `update_record`,
`delete_record`, `bulk_delete_records`), or a custom `Action` the Resource lists in `actions()`,
run on one record or on the bulk selection.

_Avoid_: Command, Mutation, Operation, Modal

### Committed

What one committed Action wrote, handed to `Resource::after_commit`: the mutation kind and the rows
it created, updated, deleted, or ran a custom action on.

_Avoid_: CommittedSet, ChangeSet, Event, PostCommit

### Detail page

`GET {prefix}/{slug}/{id}`: one record rendered read-only through `Resource::view`, followed by
`Resource::view_content` and its Relations.

_Avoid_: Show page, Infolist page, Record view

### Relation

A related Resource's rows that belong to a record, declared in `Resource::relations` with
`Relation::has_many` and rendered on the record's Detail page and edit page as the related
Resource's own Table, narrowed by a foreign key.

_Avoid_: Relation manager, Sub-table, Nested resource

### Authenticator

The trait that resolves credentials to a CurrentUser and a live Session back to it. `PasswordAuth`
is the built-in implementation over `AdminUser`; `Auth::custom(..)` installs an app's own, and
`Auth::disabled()` turns authentication off.

_Avoid_: Provider, Guard, LoginManager, AuthDriver

### CurrentUser

The signed-in identity in the request context: `{ id, login, display_name, tenant_id,
can_access_panel }`, read through `current_user(cx)` or `require_authenticated(cx)`, which answer
it only on the panel whose Session resolved it. The
Authenticator's user model never appears past it.

_Avoid_: AuthUser, Principal, Account, SessionUser

### Session

A server-side `AuthSession` row keyed by the SHA-256 hash of the token in the session cookie. It
names the panel that signed the user in, authenticates only there, and lasts seven days from
login.

_Avoid_: Token (the cookie's half), SessionStore, Login, Cookie

### Uploader

The trait that decides where a file Field's bytes go, installed once per Panel with
`Panel::uploads`. `store` returns the value the record keeps.

_Avoid_: FileStore, Attachment, Blob store

### NavigationItem

An entry in the Panel's Sidebar: a label, a NavTarget, a sort order and an optional icon, derived
from a Resource or a Page and overridable through their `navigation()`.

_Avoid_: MenuItem, NavLink, SidebarEntry

### NavTarget

Where a NavigationItem points. `Derived` leaves the URL to the Panel that mounts the Resource or
Page; `Url` is a URL its author wrote, which the Panel keeps as written.

_Avoid_: Link, Route, Target, SidebarUrl

### Notification

A short message shown to the user as a toast after an Action, in a stack the Shell owns so it
survives Table refreshes. A page can also raise one in place with `notification::live_toast`.

_Avoid_: Toast (as a domain term; the UI surface is a toast), Flash, Alert

### Streamed region

A `suspense` region whose content arrives after the first render. The resource list streams its
Table: a skeleton first, then the loaded rows. Later refreshes replace the region in place.

_Avoid_: Shard (as a domain term), Region, Island, Boundary

### EmptyState

What a content region renders when it has nothing to list (`tablo_ui::empty_state`): an icon, a
title, and optional detail and action.

_Avoid_: NoResults, Placeholder, ZeroState

### ErrorState

What a Table renders inside its Streamed region when its load fails: an icon, a title, optional
detail and a retry link. The rest of the page still renders. Distinct from EmptyState: zero rows
is a result, a failed load is not.

_Avoid_: ErrorPage, Fallback

### Shell

The layout that frames every admin page: the Sidebar, the topbar and the main content area. Each
panel registers it at its prefix; `Panel::layout` replaces it.

_Avoid_: Layout, Wrapper, Chrome

### Sidebar

The Shell's navigation region, built from Topcoat's `sidebar` Primitive. It collapses on desktop
and becomes a sheet below the `md` breakpoint.

_Avoid_: Nav, Menu, Drawer

### Page container

The standard frame of an admin page (`tablo_ui::page`): width, padding and spacing, with a header
(`page_header`) holding the title, an optional description and optional actions.

_Avoid_: Container, Wrapper, Layout

### Theme

The set of design Tokens that sets the admin's look. Tablo ships no stylesheet: each app declares
the Tokens in its own `styles.css`, and `examples/showcase/styles.css` is the reference.

_Avoid_: Skin, Style, Palette

### Token

A CSS variable, such as `--background`, `--primary` or `--border`, that components use instead of
raw colors and that switches between light and dark values.

_Avoid_: Variable, Color

### Primitive

A Topcoat UI component (button, card, select, table, input, …) copied verbatim from
`topcoat-ui-registry` into `tablo-ui/src/components/primitives/` by `cargo xtask sync-topcoat-ui`.
Never edited by hand.

_Avoid_: Component (for a synced primitive), Widget

### Component

A `#[component]` Tablo owns, in `tablo-ui/src/components/composites/` (Page container, EmptyState,
ErrorState, the theme script, Toast), composed from Primitives and Tokens.

_Avoid_: Primitive, Widget, Element, View
