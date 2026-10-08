# Vocabulary

Use these words in code, comments, issues and commits, and avoid the listed synonyms. A **Panel**
serves one **Resource** per model; each resource declares a **Table** for its list, a **Schema**
for its forms and a **Detail** for its detail page, and writes through its record functions.

| Term | Meaning | Avoid |
| --- | --- | --- |
| Panel | An admin panel under one prefix: its resources, pages, shell and authentication gate. | Admin, Dashboard, App, Site |
| Page | A panel page that is not a resource: a type implementing `Page`. | CustomPage, Screen, View |
| Resource | A type implementing `Resource` for one Toasty model. Its `Form` is `NoForm` when list-only. | Model, Entity, Collection, CRUD |
| Resource definition | The `ResourceDef` a resource declares: slug, labels, navigation, policy, tenancy, table, form, detail, relations, actions, header actions. Each panel mounts its own copy. | Config, Settings, Options |
| Query | A resource's own row scoping, such as soft deletes. | Scope, Builder |
| Table | The declaration of a list view: columns, filters, search, sort, grouping, page size. | Grid, Listing, DataTable |
| Schema | The layout of a form: layout blocks, fields, embedded values. | Form, Infolist, Fieldset |
| Detail | The declaration of a detail page: columns in layout blocks, set by `ResourceDef::view`. | Infolist, Show page |
| Column | One value read off a record: a cell of a table row, or an entry on a detail page. | Field (in a table), Cell, Attribute |
| Filter | A predicate a table adds to its query from a UI control. | Scope, Constraint, Where |
| Field | One input in a schema, bound to a model column, or to a name in an action input. | Input, Control, Widget |
| Lens | A model field named once, pairing its query path with its value reader. | Accessor, Getter, statePath |
| Relation lens | A relation field named once with `relation!`, pairing its include with its loaded-value reader; a relation column reads one. | Relation (alone), Association |
| Embedded value | A `toasty::Embed` value stored as flattened columns, bound by a form as one value. | Nested form, Sub-form, Inline model |
| Record form | The typed value a form submission parses into, one field per written column. | Patch, Draft, DTO, Form (alone) |
| Blank answer | What a record-form field stores when submitted empty. A field with none is required. | Default, Nullable (for presence) |
| Condition | What shows a field or a layout block only while another field posts one of the listed values. | Visibility rule, Dependency, Reactive field |
| Policy | A resource's authorization, answering one ability at a time. | Guard, Permission, Gate, Rule |
| Ability | One thing a policy is asked to allow. | Permission, Action, Verb |
| Tenancy | How a resource's rows belong to a tenant, through a `TenantId` column. | Tenant scope, Multi-tenancy mode |
| Relation | A related resource's rows that belong to a record, rendered as that resource's table. | Relation manager, Sub-table |
| Action | A user-invoked mutation, on one record or a bulk selection. | Command, Operation, Modal |
| Header action | A user-invoked mutation on no record, from the header of a list or a page. | Global action, Page action, Toolbar action |
| Place | Where a record action's button renders: a row, the detail page, the edit page, the bulk bar. | Location, Context, Surface |
| Action input | The typed value an action asks for before it runs, rendered on its input page. Its fields bind no column. | Action form, Parameters, Payload |
| Authenticator | The trait that loads a panel user from credentials or a session id. | Provider, LoginManager |
| Panel user | The app's own user type: id, display name, panel access, tenant memberships. | CurrentUser, AuthUser, Principal |
| Uploader | The trait that decides where a file field's bytes go, installed once per panel. | FileStore, Blob store |

Exceptions: the `AdminUser` model and the conventional `/admin` prefix keep the word "admin";
`Grid` names a Schema layout block, never a list.
