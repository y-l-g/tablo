# Resources

A resource is the admin for one Toasty model: which rows it lists, how its table and form look,
who may see and change a record, and how a write runs. You implement the `Resource` trait on a
unit struct, return what it declares from `declare()` as one `ResourceDef` value, and register it
with `Panel::resource`.

The smallest resource lists rows and nothing else:

```rust
{{#include ../../../examples/guide/src/resources.rs:audit-resource}}
```

A resource with create and edit pages names a `#[derive(RecordForm)]` struct as its `Form`. The
derive lays out the rest from the struct's fields: a table column per field a column can show,
one form control per field, and a detail page column per field. Set the def's `table`, `form` or
`view` to arrange or extend one; see [Tables](./tables.md), [Forms](./forms.md) and
[Detail pages](./detail-pages.md).

## The definition

`declare()` returns a `ResourceDef`, and every setting has a default:

| `ResourceDef` method | Default | Purpose |
| --- | --- | --- |
| `table(..)` | the record form's derived table | the list's columns, filters and options: [Tables](./tables.md) |
| `form(..)` | every control, in declaration order | how the create and edit forms arrange the record form's controls: [Forms](./forms.md) |
| `view(..)` | the record form's derived detail page | the detail page's columns; `Detail::empty()` turns the page off: [Detail pages](./detail-pages.md) |
| `record_title(lens!(..))` | the label and the record's key | the detail page's heading, each relationship option over the resource, which also searches the column, and a `RelationColumn::of` cell |
| `public_link(..)` | none | the record's public page, linked from its detail and edit pages |
| `relation(..)` | none | a related resource shown as a table on the detail and edit pages |
| `action::<A>()` | none | a record action, on rows, the detail and edit pages and the bulk bar: [Actions](./actions.md#record-actions) |
| `header_action::<A>()` | none | an action on no record, in the list's header: [Actions](./actions.md#header-actions) |
| `policy(..)` | `Deny` | what the user may do: [Policy, auth, tenancy](./policy-auth-tenancy.md#policy) |
| `tenancy(..)` | `Tenancy::none()` | how rows belong to a tenant: [Policy, auth, tenancy](./policy-auth-tenancy.md#tenancy) |
| `create_column(..)` | none | a column an overridden `create_record` sets itself, once per column |
| `slug(..)`, `label(..)`, `plural_label(..)` | from the type names | URLs and titles: [Naming](#naming) |
| `icon(..)`, `navigation_order(..)`, `navigation(..)` | the default entry | the sidebar entry: [Sidebar](./panel-and-routing.md#sidebar) |

The panel builds the def once when it mounts and serves that copy to every request.
`Panel::resource_with` adjusts it for one panel, so the same resource can mount read-only in a
second panel:

```rust
Panel::new("portal").resource_with::<PostResource>(|def| def.policy(ReadOnly))
```

## Trait methods

`Model` and `Form` are required. The methods, each with a default, receive the request:

| Method | Default | Purpose |
| --- | --- | --- |
| `query(cx)` | every row | the base query every loader starts from: [Scoping](#scoping-the-query) |
| `validate_record(cx, form)` | no errors | rules that need the whole parsed form |
| `create_record`, `update_record` | the derived write | the create and update writes: [Writes](#writes) |
| `delete_record`, `bulk_delete_records` | delete by primary key | the delete writes |
| `after_commit(cx, committed)` | nothing | side effects after a write commits |

## Naming

The names default from the type names, following Filament's conventions:

| `ResourceDef` method | Default | `BlogPostResource` over `BlogPost` |
| --- | --- | --- |
| `slug(..)` | resource name without `Resource`, pluralized, kebab-cased | `blog-posts` |
| `label(..)` | the model's type name in sentence case; used in "Create …" and "Edit …" | `Blog post` |
| `plural_label(..)` | the label pluralized; the sidebar entry and list title | `Blog posts` |

Set `label` to rename a record, and `plural_label` only when the plural rules guess wrong. Name
resources in the singular: `UsersResource` pluralizes to `userses`.

## Scoping the query

`query(cx)` is the base query of every loader: the list, the export, the edit and delete
handlers, relationship options and the detail page. Use it for the resource's own row scoping,
such as hiding soft-deleted rows:

```rust
{{#include ../../../examples/guide/src/resources.rs:soft-deleted-query}}
```

Two things do not belong in `query`:

- **The tenant filter.** For a tenant-owned resource the framework adds it to `query` at every
  loader. See [Tenancy](./policy-auth-tenancy.md#tenancy).
- **Relations.** The list, the export and the detail page load the relations their columns
  declare: a `RelationColumn` or `CountColumn` declares its own, and a `ComputedColumn` declares
  one with `include`. Include a relation in `query` only when every loader
  reads it, for example because the policy does.

In your own code, load a resource's rows with `scoped_query::<R>(cx)?`, not `R::query(cx)`:
`scoped_query` is `query` with the tenant filter of the def the request's panel mounted, and
returns an error rather than an unscoped query when the request has no tenant or the panel does
not mount `R`. A background job builds its context from the mounted panel's `PanelHandle`; see
[Outside a request](./data-access.md#outside-a-request).

The unique-value check on forms probes through the same scoped query, so a `#[unique]` index
wider than the scope is invisible to it: the check misses the collision and the database refuses
the write with a 500. Scope such indexes to match, as in `#[unique(tenant_id, email)]`.

## Writes

Every create, update and delete runs in a transaction the framework opens. For an update or a
delete, the handler first loads the target record through the scoped query inside that transaction
and checks policy on it. It then calls the resource's record function with the open transaction as
`ex`.

`create_record` and `update_record` default to writing the record form's fields
(`write_create` and `write_update`), so most resources declare neither. To check something inside
the transaction, override the function and delegate:

```rust
impl Resource for CommentResource {
    type Model = Comment;
    // …
{{#include ../../../examples/guide/src/resources.rs:comment-update-record}}
}
```

Run every statement through `ex`, and use the `record` you are given rather than loading it
again: it is the row the policy check passed. Both functions return the written row. An error
rolls the transaction back and nothing is written.

`delete_record` deletes the row by its primary key, and `bulk_delete_records` calls
`delete_record` once per record in one transaction, so overriding `delete_record` — for a soft
delete, say — covers both. A bulk delete runs like a bulk action: it deletes the selected records
the policy's `Delete` allows and reports the rest as skipped (`"Bulk deleted (2 of 5 skipped)"`).

### After the commit

`after_commit` runs once per committed write, after the transaction and before the response. Put
side effects there — email, webhooks, audit rows, cache invalidation — so a rolled-back write never
triggers them:

```rust
{{#include ../../../examples/guide/src/resources.rs:notify-after-commit}}
```

`Committed` names the mutation (`Mutation::Create`, `Update`, `Delete`, `Action(NAME)` for an
[action](./actions.md), or `Attach` and `Detach` for a
[many-to-many relation](./detail-pages.md#many-to-many-relations)) and the rows written: the
created or updated row, every deleted row in one call for a bulk delete, the rows a record action
ran on, none for a header action, or the owner whose links changed. `Mutation` is
`#[non_exhaustive]`, so a `match` on it ends with a `_` arm. The hook is not called
when nothing committed. An error it returns is logged; the write stays committed.

## Startup checks

Mounting the panel builds each resource's def once and binds its table, form and view to the
database schema, so a path through an embedded value binds its flattened column wherever a
declaration names one. It refuses the resource when:

- its table, form or view is malformed: a duplicate column, filter or field name, a zero page
  size, an empty column set (a resource whose derived table lists nothing declares its own
  `table`), or a lens that binds no column. Rendering such a table through `WiredTable::render`
  or such a schema through `Schema::render` fails with the same errors;
- the form asks for what the database or the framework will not honor: a `unique()` field with
  no unique index or whose non-nullable column an empty submission would fill, or a tenant-owned
  resource's record form claiming its tenant column;
- a relationship field takes its options from a resource the panel does not register, or a
  `RelationColumn::of` labels its records by one;
- a choice field declares neither options nor a relationship, so its `<select>` offers nothing;
- the policy allows `Create` and a non-nullable column is set by nothing: not the form, not a
  Toasty default, not the tenant stamp, and not named by `create_column`;
- a `NoForm` resource's policy allows `Create`;
- a `Tenancy::column` lens is not one field of the model, a `Tenancy::via` lens is, or the form
  of a `Tenancy::via` resource writes the parent's foreign key other than through a relationship
  field over a tenant-scoped resource;
- two actions share a `NAME`, record or header;
- a many-to-many field or relation goes through a join model a link cannot write, or a multiple
  choice in a resource's form names no many-to-many field;
- a relation names a resource the panel does not register, or names one twice;
- the panel registers the resource twice.

The refusal lists every mistake the panel found, not only the first. `.panel(..)` returns it as a
`MountError`: each of its `DeclarationError`s names the resource, the `Site` of the declaration
it is in (`Registration`, `Table`, `Form`, `View`, `Tenancy`, a `Relation`) and a
`DeclarationErrorKind`, whose `Display` is the message.
[Testing](./testing-and-benchmarks.md#testing-a-panel) shows a test matching on the kind.

An action's `NAME` is checked when the app compiles: `ResourceDef::action` and
`ResourceDef::header_action` do not compile an action whose name is not one URL segment.

A modifier on the wrong kind of field does not compile: each `Field` constructor returns its
control's builder (`TextField`, `ChoiceField`, `FileField`, `CustomField`), which offers only
that control's modifiers.

A form control from elsewhere does not compile either: `ResourceDef::form` takes a
`Schema<R::Form>`, which places only the record form's own `controls()`, so a control of another
form, or a field built with `Field::text`, is a type error rather than a refused mount.

The panel serves the checked def to every request, so `declare()` must not depend on the request:
it takes no user, tenant or query string. Request-dependent decisions belong to the policy and to
the trait methods, which receive `cx`.
