# Tablo

Admin toolkit for Rust, server-rendered on Topcoat and persisted with Toasty. A **Panel** serves
one **Resource** per model; each resource declares a **Table** for its list and a **Schema** for
its forms and detail page, and writes through its record functions.

This file defines the project's vocabulary: use these words in code, comments, issues and commits,
and avoid the listed synonyms. It defines words only. Behaviour lives in the [user
guide](docs/guide/src/introduction.md) and in rustdoc; decisions live in [`docs/adr/`](docs/adr/).

## Language

### Panel

An admin panel under one prefix: its resources, pages, shell, and authentication gate.

_Avoid_: Admin, Dashboard, App, Site

_Documented exceptions_: the `AdminUser` model keeps the `Admin` prefix, and `/admin` is the
conventional mount prefix, not Panel vocabulary.

### Page

A panel page that is not a Resource: a type implementing the `Page` trait.

_Avoid_: CustomPage, Screen, View

### Resource

A type implementing `Resource` for one Toasty model: its resource definition, its query, and its
record functions. A resource whose `Form` is `NoForm` is list-only.

_Avoid_: Model, Entity, Collection, AdminModel, CRUD

### Resource definition

What a resource declares, as one `ResourceDef` value: its slug, labels, navigation, policy,
tenancy, table, schemas, relations, and actions. Each panel mounts its own copy.

_Avoid_: Config, Settings, Options (as a domain term)

### Query

A resource's own row scoping, such as soft deletes.

_Avoid_: Scope, EloquentQuery, Builder (as a domain term)

### Table

The declaration of a resource's list view: its columns, filters, search, sort, grouping, and
page size.

_Avoid_: Grid, Listing, DataTable

_Documented exception_: `Grid` is also a Schema layout block. Only the layout block uses the
name; the rendered list is a Table everywhere, including comments and local variables.

### Schema

The layout of a form or a detail page: layout blocks, fields, and embedded values.

_Avoid_: Form, Infolist, Fieldset (as a top-level term), statePath

### Column

One cell of a table row, rendered from the record.

_Avoid_: Field (in a table), Cell, Attribute

### Filter

A predicate a table adds to its query from a UI control.

_Avoid_: Scope, Constraint, Where

### Field

One input in a schema, bound to a model column.

_Avoid_: Input, Control, Widget (in a form), statePath

### Lens

A model field named once, pairing its query path with its value reader.

_Avoid_: Accessor, Getter, statePath

### Embedded value

A `toasty::Embed` value stored in its parent's row as flattened columns, bound by a form as one
value rather than column by column.

_Avoid_: Nested form, Sub-form, Composite field, Inline model

### Record form

The typed value a resource's form submission parses into, with one field per model column the
form writes.

_Avoid_: Patch, Draft, Input, DTO, Form (alone: that is the Schema)

### Blank answer

What a record-form field stores when its control is submitted empty. A field with none is
required; the record form is the only place a field's presence is declared.

_Avoid_: Default, Optional control, Nullable (for presence)

### Policy

A resource's authorization, answering one ability at a time.

_Avoid_: Guard, Permission, Gate, Rule

### Ability

One thing a policy is asked to allow.

_Avoid_: Permission, Action (an Action is a custom mutation), Verb

### Tenancy

How a resource's rows belong to a tenant. The column is typed `TenantId`.

_Avoid_: Tenant scope, Multi-tenancy mode

### Relation

A related resource's rows that belong to a record, rendered as the related resource's own table.

_Avoid_: Relation manager, Sub-table, Nested resource

### Action

A user-invoked mutation, on one record or on the bulk selection.

_Avoid_: Command, Operation, Modal

### Authenticator

The trait that loads a panel user, from credentials at login and from a session id on each
request.

_Avoid_: Provider, Guard, LoginManager, AuthDriver

### Panel user

The app's own user type: its id, display name, panel access, and tenant memberships.

_Avoid_: CurrentUser, AuthUser, Principal, Account, SessionUser

### Uploader

The trait that decides where a file field's bytes go, installed once per panel.

_Avoid_: FileStore, Attachment, Blob store
