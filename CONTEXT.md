# Vocabulary

Use these words in code, comments, issues and commits, and avoid the listed synonyms. A **Panel**
serves one **Resource** per model; each resource declares a **Table** for its list, a **Schema**
for its forms and a **Detail** for its detail page, and writes through its record functions.

| Term | Meaning | Avoid |
| --- | --- | --- |
| Panel | An admin panel under one prefix: its resources, pages, shell and auth gate. | Admin, Dashboard, App, Site |
| Auth gate | The panel's check, ahead of every handler under its prefix and Topcoat's runtime path, that the request comes from a signed-in user with panel access; `auth::guard` makes it in an app's own handler. | Guard (alone), Middleware, Firewall |
| Page | A panel page that is not a resource: a type implementing `Page`. | CustomPage, Screen, View |
| Page definition | The `PageDef` a page declares: slug, label, navigation, access, header actions, widgets. Each panel mounts its own copy. | Config, Settings, Options |
| Widget | One block of a page's or a resource list's widget grid, a type implementing `Widget`: a row of stats, a chart, a list of recent records. | Card, Tile (for the block), Panel (alone) |
| Stat | One figure in a row of stats: a label, a value, and optionally a detail line, a trend and a sparkline. | KPI, Metric, Card |
| Chart | A line or bar chart a widget draws: labels along the bottom and a series of values per thing it measures. | Graph, Plot |
| Record table | A few of a resource's records a widget lists under the resource's list columns, linking each record and the full list. | Table widget, Recent records |
| Resource | A type implementing `Resource` for one Toasty model. Its `Form` is `NoForm` when list-only. | Model, Entity, Collection, CRUD |
| Resource definition | The `ResourceDef` a resource declares: slug, labels, navigation, policy, tenancy, table, form, detail, relations, actions, header actions. Each panel mounts its own copy. | Config, Settings, Options |
| Query | A resource's own row scoping, such as row visibility. | Scope, Builder |
| Soft delete | A delete that stamps the column `ResourceDef::soft_delete` names with the time and keeps the row; the stamped record is trashed until a restore empties the column or a force delete removes the row. | Archive, Recycle bin, Paranoid |
| Trashed record | A record a soft delete stamped: every loader leaves it out unless a `Trashed` scope asks for it. | Deleted record, Archived record |
| Field access | Who may view and who may change one field of a model's records within a panel, declared by `ResourceDef::field_access` on a resource over the model: a request that may not view it sees no column, filter, detail entry, export column, relation column or control for it, on any resource the panel mounts over the model. | Field permission, Column security, Field-level authorization |
| Import | Creating one record per row of a CSV file through a resource's create form, declared by `ResourceDef::importable`: every row writes in one transaction, or none does. | Upload, Bulk create, Importer |
| Global search | The search box in a panel's topbar over every resource it mounts, and its results page: each resource's matching records, as its list shows them to the user, under its plural label. | Spotlight, Omnisearch, Command palette, Quick search |
| Table | The declaration of a list view: columns, filters, tabs, search, sort, grouping, page size. | Grid, Listing, DataTable |
| Schema | The layout of a form: layout blocks, fields, embedded values. | Form, Infolist, Fieldset |
| Detail | The declaration of a detail page: columns in layout blocks, set by `ResourceDef::detail`. | Infolist, Show page |
| Column | One value read off a record, shown down a table or as an entry on a detail page; one row's value of it is a cell. | Field (in a table), Attribute |
| Column choice | The toggleable columns one signed-in user shows or hides in one resource's list, against their defaults; a `ColumnPreference` row keeps it. | Column manager, Column visibility, Layout |
| Filter | A predicate a table adds to its query from a UI control. | Scope, Constraint, Where |
| List tab | One of the views a table offers above its rows, a predicate `?tab=` picks by its key, and optionally the count of its rows. | Preset, Saved view, Scope, Filter tab |
| Field | One entry of a schema, bound to a model column or to a name in an action input, and rendered as a control. | Input, Widget |
| Control | What a field renders and posts: a text input, a select, radios or buttons, a toggle or switch, a file input, a rich text editor, or a type implementing `Control`. | Widget, Input (for the field) |
| Rich text | HTML a field edits through a toolbar and stores in a `String` column, holding only the elements its toolbar's tools produce. | WYSIWYG, Rich editor, Markdown, Content |
| Tool | One button of a rich text field's toolbar, and the elements it lets the field keep. | Toolbar button, Extension, Format |
| Lens | A model field named once, pairing its query path with its value reader. | Accessor, Getter, statePath |
| Relation lens | A relation field named once with `relation!`, pairing its include with its loaded-value reader; a relation column reads one. | Relation (alone), Association |
| Embedded value | A `toasty::Embed` value stored as flattened columns, bound by a form as one value. | Nested form, Sub-form, Inline model |
| Repeater | A field editing a `#[document]` list of embedded structs: a row of the item's controls per item, which the browser adds, removes and moves. | Builder, List field, Array field, Table field |
| Model columns | The table and detail page a model reads as when its resource declares neither, from `ModelColumns`: a column per field a person reads. | Infolist, Default view, Read model |
| Record form | The typed value a form submission parses into, one field per written column. | Patch, Draft, DTO, Form (alone) |
| Form values | What a form holds, one value per key in the shape its control posts: text, a multiple choice's list, a repeater's rows. A record form hydrates a record into them and parses a submission from them. | Form data, Params, Input map |
| Blank answer | What a record-form field stores when submitted empty. A field with none is required. | Default, Nullable (for presence) |
| Join model | A model whose rows link two other models' records, one row per pair, with a `belongs_to` to each. | Pivot, Junction table, Bridge table |
| Many-to-many field | A record-form field holding the keys of the records a join model links the record to, edited as a multiple choice. | Multi-select, BelongsToMany field, Tags field |
| Dependent choice | A relationship choice offering only the related records whose column equals another field's value. | Cascading select, Chained select, Linked field |
| Condition | What shows a field or a layout block only while another field posts one of the listed values. | Visibility rule, Dependency, Reactive field |
| Policy | A resource's authorization, answering one ability at a time. | Guard, Permission, Gate, Rule |
| Ability | One thing a policy is asked to allow. | Permission, Action, Verb |
| Tenancy | How a resource's rows belong to a tenant, through a `TenantId` column. | Tenant scope, Multi-tenancy mode |
| Relation | A related resource's rows that belong to a record, rendered as that resource's table: its rows holding the record's key, or the records a join model links the record to, which the table attaches and detaches. | Relation manager, Sub-table |
| Action | A user-invoked mutation, on one record or a bulk selection. | Command, Operation, Modal |
| Change | What one write of a resource wrote, as `before_commit` and `after_commit` receive it: the kind of write and the rows it names. | Event, Committed |
| Refusal | A submission the panel answers by rendering its form again with an error under each field it names, writing nothing: a parse, rule, `validate_record` or `validate_input` error, or a record fn's or an action's `WriteError::Refused`, which also rolls back what the fn wrote. An import row with such errors is a refused row. | Rejection, Validation failure |
| Outcome | What an action's run asks of the response once its write commits: the notification it flashes and where it lands. | Result, Response, Effect |
| Notification | A message for a panel user: a status, a title, a description and links. A write flashes one as a toast; `Notifications` stores one. | Alert, Message, Flash (for the type) |
| Stored notification | A notification kept for one panel user on one panel, for one tenant or none, as a `StoredNotification` row; the bell lists it. | Database notification, Inbox item |
| Bell | The topbar control `Panel::notifications` shows: the unread count, and the sheet listing the user's stored notifications. | Inbox, Notification center, Drawer |
| Header action | A user-invoked mutation on no record, from the header of a list or a page. | Global action, Page action, Toolbar action |
| Place | Where a record action's button renders: a row, the detail page, the edit page, the bulk bar. | Location, Context, Surface |
| Form input | A typed value a form posts under keys no column binds, from `FormInput`: an action input, a sign-up, a repeater's item. | Input (alone), Params, Form object |
| Action input | The typed value an action asks for before it runs, in a dialog over the page, or on its input page after a refusal. Its fields bind no column. | Action form, Parameters, Payload |
| Authenticator | The trait that loads a panel user from credentials or a session id. | Provider, LoginManager |
| Registrar | The trait that creates a panel user from the sign-up page's input. | Registration handler, SignupProvider, UserCreator |
| Profile editor | The trait that lets the signed-in panel user edit their own account on the profile page. | Account settings, ProfileProvider, UserUpdater |
| Password resetter | The trait that finds an account by email and sets its password when a mailed reset link is used. | Password broker, ResetProvider |
| Email verifier | The trait that says whether a panel user proved they read their address, and records an address a mailed link proved. | Verification provider, MustVerifyEmail |
| Tenant registrar | The trait that creates a tenant, with the signed-in panel user a member of it, from the tenant registration page's input. | Team creator, TenantProvider |
| Tenant editor | The trait that lets an admitted member edit the tenant the request acts for on the tenant settings page. | Team settings, TenantProvider |
| Auth mail | The sender and base URL a panel's auth mails its links from, set by `Auth::mail`. | Mailer, Notifier |
| Auth token | A single-use secret a mailed link, a mailed code or a pending sign-in carries, stored as an `AuthToken` row holding its hash. | Nonce, Signed URL, Reset token |
| Two-factor sign-in | Signing in with a code from an authenticator app or mailed to the account, or a recovery code, after the password, set by `Auth::two_factor`. | MFA, 2FA, TOTP |
| Auth factor | A second factor an account holds on a panel: its sealed authenticator secret, its choice of codes by email, or one recovery code's hash, stored as an `AuthFactor` row. | Two-factor secret, Backup code |
| Panel user | The app's own user type: id, display name, panel access, tenant memberships. | CurrentUser, AuthUser, Principal |
| Time zone | The zone a request shows and reads wall-clock times in: the panel user's, else the panel's, else UTC. A timestamp's control, its date column and its date filter use it. | Timezone, TZ, Locale |
| Locale | The language a request reads its panel in, a `topcoat::l10n::Locale`: the panel user's, else the browser's best match among the panel's locales, else the panel's default. | Language (for the setting), Culture, Lang |
| Catalog | One locale's Fluent messages: Tablo's, under the app's from `Panel::translations`. | Dictionary, Translation file, Lang file |
| Message | One entry of a catalog, named by its id, with the variables its placeables read. | Key, String, Translation key |
| Text | A word a panel renders: plain, as written, or a message the request's locale spells. | Label (for the type), LocalizedString, Translatable |
| File list | A file field over a `Vec<String>` column, from `Field::files`: a row per stored path, each removable, and an input adding several files at once. | Multiple upload, Gallery, Attachments field |
| Uploader | The trait that decides where a file field's bytes go, installed once per panel. | FileStore, Blob store |
| DirUploader | The stock `Uploader`: writes each file into one served directory and stores its URL. | DiskStore |

Exceptions: "admin" describes a panel ("an admin panel", "an admin toolkit") but never names one;
the `AdminUser` model and the conventional `/admin` prefix keep the word. `Grid` names a Schema
layout block, never a list. "Modal" names only the browser's `showModal`; the thing it opens is a
dialog.
