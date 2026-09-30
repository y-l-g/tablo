# Tablo

Admin toolkit for Rust — server-rendered on Topcoat, persisted with Toasty. Provides the CRUD
core of Filament (Panel + Resource → Table + Schema, deletes via Resource record fns) with no
Livewire port, explicit preloading and cursor pagination, and a narrow reactivity seam: streamed
`suspense` regions render the list shell first and swap the loaded table in, while reruns morph in
place (focus survives) and tables opting into `Table::live_search` re-render their table in place
through the slug-dispatched `table_search` shard: search, sort, filters, and pagination write
signals, and the table morphs without a navigation (ticket #104, GH #151). A confirmed delete rides
the same seam: the POST still 303s, and the client that follows it re-runs the table's shard instead
of morphing the response (GH #234, ADR-0020).

> **Shipped vs spec:** everything the terms below call shipped — Panel, Resource, Table, Schema,
> Policy, authentication, tenancy, and uploads — lives in `tablo-core`. `examples/showcase` is
> the runnable reference and `docs/guide` the user guide for what each seam does in detail. A term
> marked spec-level or future work is not implemented.

## Language

### Panel

The admin application. Owns the Router, the Db in app_context, the layout Shell, its declared
Resources and Pages, and the default-on authentication gate (ADR-0013). Declaring a Panel with
Resources and Pages yields their routes and navigation; an app's layout delegates to
`Panel::layout_shell` for the Shell with no manual document HTML.

_Avoid_: Admin, Dashboard, App, Site

_Documented exceptions_: shipped `AdminUser` model retains the `Admin` prefix (auth seam,
ADR-0013); the `/admin` mount default is generic English for the URL prefix, not Panel vocabulary.

### Authenticator

The one authentication seam (ADR-0013). An object-safe trait resolving credentials into the erased
`CurrentUser` and a live session back to it; `PasswordAuth` is the shipped default over
`AdminUser`, `Panel::auth(Auth::custom(..))` swaps in an app implementation over its own user
table, and `Auth::disabled()` is the explicit, greppable opt-out. Sessions stay framework-owned
(`AuthSession`) whichever implementation is in use.

_Avoid_: Provider, Guard, LoginManager, AuthDriver

### CurrentUser

The erased identity resolution places in request `Cx`:
`{ id, login, display_name, tenant_id, can_access_panel }`. Pages, shards, and app code read it
only through `current_user(cx)` / `require_authenticated(cx)`; the concrete user model never leaks
past the `Authenticator`. The optional `tenant_id` becomes the request `Tenant`.

_Avoid_: AuthUser, Principal, Account, SessionUser

### Session

A server-side `AuthSession` row keyed by the SHA-256 hash of a client token carried in a hardened
cookie (`__Host-`, HttpOnly, Secure, SameSite=Lax). Seven-day fixed lifetime, rotated on login,
deleted on logout, revocable per user, and swept on a successful login (up to 500 expired rows), so a
row whose owner never returns does not stay forever; the raw token is never stored.

_Avoid_: Token (the client half), SessionStore, Login, Cookie

### Resource

A type that maps one Toasty Model to its admin UI: the base query, the table, the view
(GH #187) and its values (`view_values(cx, record)`), the navigation entry, and the policy
(`can_*` predicates), and its form: the record form (`type Form`), the form's schema (`form()`),
`validate_record`, and the create and update record fns. A list-only resource names
`type Form = NoForm<Self::Model>;`. One Model → one Resource, registered once with
`Panel::resource`: every resource gets the list, detail, delete, bulk delete, and export routes,
and a resource with a record form also gets create, edit, and the relationship options — not a
`pages()` declaration. A resource that declares no `view` has no detail page: `viewed()` is
derived from the schema, so the route and the row's `View` link cannot disagree. Tenancy,
export scoping, and row chrome are covered in the guide; see
[resources](docs/guide/src/resources.md) and
[policy, auth, tenancy](docs/guide/src/policy-auth-tenancy.md).

_Avoid_: Model, Entity, Collection, AdminModel, CRUD

### Schema

The unified layout primitive for forms, infolists, and detail pages (GH #187, ADR-0016): one
vocabulary read two ways — `render(cx, Source::form(..))` gives controls, `render(cx,
Source::view(..))` gives the record's stored values under the same labels and layout. A composition
of layout blocks (Section, Group, Grid, Repeater), embedded values, and Fields bound via field lenses
to a Model; building it resolves every Field once into one field list. See
[forms](docs/guide/src/forms.md) and [detail pages](docs/guide/src/detail-pages.md).

_Avoid_: Form, Infolist, Fieldset (as top-level term), statePath

### Table

The declarative description of a list view. Declares columns, filters, search, sort, pagination,
and row/bulk actions. It also declares how to query — searchable and filterable columns produce
Toasty predicates, sortable columns map to order_by. Owns the row loop: row identity is mandatory
and typed, declared once via the constructor (`Table::new(|u| u.id.to_string(), columns)`)
until Toasty exposes instance→PK extraction — never a loop index.
Identity is two projections: the display key (keyed diffs, DOM ids) and the
record key (edit/delete URLs, bulk checkbox values), resolved by handlers as the
typed PK. A table whose display projects a non-PK value uses
`Table::new_split(display, record, columns)`.

Row chrome follows the resource's declarations (`TableChrome`, GH #383) and is gated per record by the table's
**row policy**, which the panel wires from the resource's
`can_view`/`can_update`/`can_delete`: a refused row renders no link, and a delete-refused row no
bulk checkbox. The handler's all-or-nothing check stays as
the safety net for a hand-crafted POST.

A `live_search()` table hands its chrome to the page's `TableSignals`: the shard's tracked
reads re-render the table in place when search, sort, filters, or pagination write a signal
(GH #151). See [tables](docs/guide/src/tables.md) and ADR-0003.

_Avoid_: Grid, Listing, DataTable

_Documented exception_: `Grid` is also a shipped **Schema layout block** — `Grid::new(2)`, a column
container for forms and detail pages (ADR-0016). It is a different artifact from the list view, and
only the layout block keeps the name; the rendered list is a Table everywhere, including in code
comments and locals.

### Column

A typed projection of a Model field (or a computed value) displayed in a Table row, rendered through
a lens-bound closure where typos fail at compile time. `searchable`/`sortable` map to Toasty
predicates and order_by; computed columns render values but declare none. A column whose projection
reads a relation declares it with `include(..)`, a typed path (ADR-0018). A column declares its
width with `width(ColumnWidth::..)`; see [tables](docs/guide/src/tables.md) (GH #240). `TextColumn`
is the only column type; Badge, Number and the rest remain spec-level.

_Avoid_: Field (in table context), Cell, Attribute

### Detail page

`GET {prefix}/{slug}/{id}` (GH #187): one record in two halves — `Resource::view`'s Schema,
read-only, plus `Resource::view_relations(cx, record)` for the related rows the query's `include`
loaded. The Schema renders the record's string projection, so a relation (a list of records) needs
the typed half; loading goes through the tenant-scoped query (`scoped_query`, GH #223) like every
other record page, so an unknown id and one outside the request's scope are the same 404, while a
record the caller may not view is a 403.

_Avoid_: Show page, Infolist page, Record view

### Action

A user-invoked delete/create/edit operation driven by a record fn (`Resource::delete_record` /
`bulk_delete_records`, `create_record` / `update_record`) through a POST handler, inside a
transaction, with authorization checked against the passed record inside the handler (ADR-0004).
A non-CRUD operation (publish, archive) is still modelled as a record fn or a Page;
an `Action` value with its own before/after hooks remains future work (GH #112).

_Avoid_: Command, Mutation, Operation, Modal

### Committed

What one successful mutation wrote, handed to `Resource::after_commit` (GH #112, ADR-0004):
the mutation kind plus the rows it wrote — the row a create or update returned, the rows a
delete or bulk delete removed (gone by the time the hook sees them, so they arrive as they
were). One `Committed` per write. The hook runs after `tx.commit()` and before the response;
a failed hook is logged and never rolls the write back.

_Avoid_: CommittedSet, ChangeSet, Event, PostCommit

### Query

The base filtered query for a Resource, returned by `Resource::query(cx)` and used by every
loader through `scoped_query(cx)` — that base with the framework's tenant filter ANDed on when
the resource requires a tenant (GH #223). The list and the CSV export add the relations their
columns include, and the detail page loads `Resource::view_query(cx)` (ADR-0018). See
[data access](docs/guide/src/data-access.md).

_Avoid_: Scope, EloquentQuery, Builder (as domain term)

### Policy

The per-Resource authorization rules, implemented as `Resource::can_view_any`/`can_view`/
`can_create`/`can_update`/`can_delete_any`/`can_delete` — the one authorization vocabulary.
Default-deny; checked in both page and POST handlers, and in relationship option loads (GH #108).
The row chrome follows the declarations that also govern each route (GH #383): the row Delete
control, the bulk column, and the bulk bar render when `can_delete_any` allows, the Edit link when
the resource has a record form, and the View link when `Resource::viewed` holds. The panel then
applies `can_view` + `can_update` / `can_delete` per row through the table's row policy (GH #235):
a refused row renders no link and an empty bulk cell. `can_delete` defaults to `can_delete_any`.
See [policy, auth, tenancy](docs/guide/src/policy-auth-tenancy.md).

_Avoid_: Guard, Permission, Gate, Ability, Policy trait

### NavigationItem

An entry in the Panel sidebar: a label, a NavTarget, and a sort order. Derived by default from a
Resource or a Page, overridable to change the label, the order, or an explicit URL. The Panel that
owns the entry owns the URL: a `Derived` target names none, so the panel resolves it from its own
mount prefix plus the slug it mounts the resource or page at (the prefix itself for the home page),
while an explicit URL is a link its author wrote and is kept
verbatim (GH #165).

_Avoid_: MenuItem, NavLink, SidebarEntry

### NavTarget

Where a NavigationItem points. `Derived` means the declaring Resource or Page cannot know its
mount, so the owning Panel resolves the URL; `Url` names a URL outright. The distinction is the type rather
than a convention, so prefix resolution can never touch a URL an author wrote (GH #165).

_Avoid_: Link, Route, Target, SidebarUrl

### Filter

A predicate contributed to a Table's query. A typed wrapper around a Toasty `Expr<bool>` produced
from a UI control (SelectFilter, TernaryFilter, DateFilter, VariantFilter), composed with AND.

_Avoid_: Scope, Constraint, Where

### Field

One input bound to a Model lens inside a Schema: `Field::text`, `Field::choice`, or `Field::file`,
one type whose control is text (multi-line with `.multiline(rows)`), choice, or file. `Field::text`
binds any `FormScalar` lens — `String`, a `TypedValue` type (`i64`, `Uuid`, `jiff::Timestamp`), or an
`Option` of one (GH #192) — rendering the value's spelling and parsing the submission through the
type. A constructor takes a column's lens or a `ResolvedLens`, which binds an embedded leaf to its
flattened column. `required` defaults from Toasty column nullability (opt out with `.optional()`),
and `unique()` on a non-nullable column implies **presence** (GH #189): an empty `String` stores
`""` (GH #89), which a unique index admits once; an `Option` column stores NULL. Renders through the upstream `field`
family (topcoat#420). See [forms](docs/guide/src/forms.md) and ADR-0001.

_Avoid_: Input, Control, Widget (in form context), statePath

### Embedded value

A `toasty::Embed` struct or enum stored in the parent row's flattened columns, bound as a **value**
rather than leaf by leaf (GH #191, ADR-0019). `#[derive(EmbeddedForm)]` generates the flat-map ↔
typed conversion and a `form(cx, parent)`: one schema node holding the value's resolved fields,
whose keys are its fields' keys. An enum's variant is its **discriminant column**: hydration writes
the stored variant into a visible choice, so a variant can be picked on create and changed on edit.
A field marked `#[form(embed)]` is a nested value, every other field a scalar. Per-field overrides
are `#[form(label = "…")]` and `#[form(multiline = N)]`; an unknown key is a compile error. See
[forms](docs/guide/src/forms.md).

_Avoid_: Nested form, Sub-form, Composite field, Inline model

### Record form

The typed value a resource's form submission parses into (GH #369, ADR-0022):
one struct, `#[derive(RecordForm)]`, with one field per model column the form writes, named
and typed like the model's field. A scalar binds the key its control posts; an embedded value
(`#[form(embed)]`) binds every key it occupies. The panel parses every submission into it,
hydrates the edit and detail pages from it, and writes it through toasty's builders. See
[forms](docs/guide/src/forms.md#the-record-form).

_Avoid_: Patch, Draft, Input, DTO, Form (alone: that is the `Schema`)

### Completion

On an edit, filling every declared key the submission did not post from the stored record
before the form parses, so the record fn receives a whole form and an unposted key keeps its
value. The **named** keys are the ones the submission posted; a **named field** has at least one
named key, and the write assigns only named fields (plus the model's `#[update(..)]` defaults and
`#[version]` bump). An untouched file input is not named.

_Avoid_: Presence, Backfill (as the general term), Merge, Default

### Posted

What `Resource::update_record` receives: the parsed record form (it derefs to it) and the
fields the submission named. `into_update(&mut record)` is the instance update builder with one
assignment per named field, or `None` when the submission named none.

_Avoid_: Patch, Changes, Diff, Submission

### Uploader

Where a file field's bytes go (GH #188, ADR-0017): a trait the app implements and installs once
per Panel (`Panel::uploads`). `store(filename, bytes) -> Result<String, String>` receives the
part's already-sanitized basename and returns the value the record stores; a refusal
(`Err(reason)`) is an inline field error. With no uploader installed the sanitized basename is
stored, and the bytes are drained rather than buffered. `Panel::serve_dir(path, dir)` mounts an
app-owned filesystem directory on the panel's router; a served directory is **public**. See
[forms](docs/guide/src/forms.md).

_Avoid_: FileStore, Attachment, Blob store

### Streamed region

A `suspense` region of the page whose content swaps in after the first render. The resource list
streams its table: a skeleton of the table's own header and pulsing rows first, then the loaded
rows swap in without a client library. Later reruns (page/shard) morph in place per Topcoat #392 — focus, scroll, and typing
survive. The table always renders inside a `data-boundary` region. See ADR-0003.

_Avoid_: Shard (as domain term), Region, Island, Boundary

### Notification

A transient user-visible message (status + title + optional description, rendered as a
shadcn/Sonner toast, auto-dismissed after ~4s by `notifications.js` with a close button) produced
by a record operation's result, rendered in a shell-level stack owned by the Panel layout so it
survives table swaps. A page can also mount one in place — `notification::live_toast` signals plus
the shell's `live_toaster` shard — so a procedure's result becomes a toast without a navigation
(GH #154 §3).

_Avoid_: Toast (as domain term; the shadcn UI surface is a toast), Flash, Alert

### EmptyState

The Table's zero-rows rendering (icon + title + optional action), shown for "no records" and "no
search results".

_Avoid_: NoResults, Placeholder, ZeroState

### ErrorState

The Table's failed-load rendering: a destructive-accented block (icon + title + optional detail +
retry action) shown **inside** the streamed region when the load `Err`s — the load catches its own
error so the page shell survives and the body is not truncated (GH #79). Distinct from EmptyState:
zero rows is a result, a failed load is not.

_Avoid_: ErrorPage, Fallback

### Shell

The top-level layout that frames every admin page. Owns the Sidebar, topbar, and main content area.

_Avoid_: Layout, Wrapper, Chrome

### Sidebar

The persistent navigation region inside the Shell. The upstream Topcoat `sidebar` primitive (synced
into `tablo-ui`, topcoat#419): header, content, footer, groups and menus, collapsing to
offcanvas on desktop and to its own sheet drawer below `md`. Its open state is runtime signals —
`Panel::render_shell` seeds `open` from the `sidebar_state` cookie, the trigger pair carries
`@click` handlers, and `assets/sidebar.js` mirrors changes back to the cookie.

_Avoid_: Nav, Menu, Drawer

### Page

A panel page that is not a Resource: a type implementing `tablo_core::Page`, registered with
`Panel::page` at `{prefix}/{slug}` or with `Panel::home` at the prefix itself. The Panel owns its
route and its NavigationItem; the page owns its markup, which the Shell frames like every panel
page.

_Avoid_: CustomPage, Screen, View

### Page container

The standard container for an admin page (`tablo_ui::page`). Owns max-width, padding and vertical rhythm so pages
declare title and content, not Tailwind layout classes.

_Avoid_: Container, Wrapper, Layout

### Theme

The named set of design tokens that determines the admin's look. Tablo provides **no stylesheet**:
the tokens are the app's, declared in its `styles.css` as the per-app contract of ADR-0006, and
`examples/showcase/styles.css` is the reference — a neutral set that re-tunes upstream's
`--primary`/`--ring`. The one theme component `tablo-ui` owns is `theme_init_script`, a free
function (not a `Theme` type) that reconciles the `dark` class before first paint;
`Panel::dark_mode` supplies its fallback for a visitor with no stored choice.

_Avoid_: Skin, Style, Palette

### Token

A CSS variable (such as `--background`, `--primary`, `--border`) that components reference instead
of raw colors, swapping between light and dark values.

_Avoid_: Variable, Color

### Primitive

A re-exported Topcoat UI component (button, card, select, table, input...) vendored verbatim from
`topcoat-ui-registry` into `tablo-ui/src/components/primitives/` and synced via
`cargo xtask sync-topcoat-ui`.

_Avoid_: Component (when meaning synced primitive), Widget

### Component

An owned Topcoat `#[component]` in `tablo-ui/src/components/composites/` (Page container,
ErrorState, Theme, Toast) that composes Primitives and Tokens. Hand-written, never overwritten by sync.

_Avoid_: Primitive, Widget, Element, View
