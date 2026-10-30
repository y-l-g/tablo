# Resources

A resource declares the panel's pages for one Toasty model: which rows it lists, how its table
and form look, who may see and change a record, and how a write runs. You implement the
`Resource` trait on a unit struct, return what it declares from `declare()` as one `ResourceDef`
value, and register it with `Panel::resource`.

The smallest resource lists rows and shows each one, and writes nothing:

```rust
{{#include ../../../examples/guide/src/resources.rs:audit-resource}}
```

The model derives `ModelColumns`, which reads what a person sees: a table column and a detail page
column per field a column can show, whoever writes it. A resource with create and edit pages names
a [record form](./forms.md) as its `Form`, which writes: the one `#[derive(ModelForm)]` writes for
the model, or a `#[derive(RecordForm)]` struct, with one form control per field. Set the def's
`table`, `form` or `detail` to arrange or extend one; see [Tables](./tables.md),
[Forms](./forms.md) and [Detail pages](./detail-pages.md).

## The definition

`declare()` returns a `ResourceDef`, and every setting has a default:

| `ResourceDef` method | Default | Purpose |
| --- | --- | --- |
| `table(..)` | the model's `ModelColumns::table()` | the list's columns, filters and options: [Tables](./tables.md) |
| `form(..)` | every control, in declaration order | how the create and edit forms arrange the record form's controls: [Forms](./forms.md) |
| `detail(..)` | the declared table's columns, else the model's `ModelColumns::detail()` | the detail page's columns: [Detail pages](./detail-pages.md) |
| `without_page(ResourcePage::..)` | the list, the view page, and the create and edit pages of a resource with a record form | turns the create or the view page off; the policy decides who may open a page, never whether it exists |
| `record_title(lens!(..))` | the label and the record's key | the detail page's heading, each relationship option over the resource, which also searches the column, each [global search](./panel-and-routing.md#global-search) result, and a `RelationColumn::of` cell |
| `global_search(..)`, `without_global_search()` | searched, each result titled alone | the details under each result in the panel's [global search](./panel-and-routing.md#global-search), or the resource left out of it |
| `public_link(..)` | none | the record's public page, linked from its detail and edit pages |
| `relation(..)` | none | a related resource shown as a table on the detail and edit pages |
| `action::<A>()` | none | a record action, on rows, the detail and edit pages and the bulk bar: [Actions](./actions.md#record-actions) |
| `header_action::<A>()` | none | an action on no record, in the list's header: [Actions](./actions.md#header-actions) |
| `policy(..)` | `Deny` | what the user may do: [Policy, auth, tenancy](./policy-auth-tenancy.md#policy) |
| `tenancy(..)` | none: served to every tenant, which mounting refuses over tenant-owned rows | how rows belong to a tenant: [Policy, auth, tenancy](./policy-auth-tenancy.md#tenancy) |
| `create_column(..)` | none | a column an overridden `create_record` sets itself, once per column |
| `slug(..)`, `label(..)`, `plural_label(..)` | from the type names | URLs and titles: [Naming](#naming) |
| `icon(..)`, `navigation_order(..)`, `navigation_group(..)`, `navigation(..)`, `without_navigation()` | the default entry | the sidebar entry; `without_navigation()` leaves it out: [Sidebar](./panel-and-routing.md#sidebar) |

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
| `validate_record(cx, form)` | no errors | rules that need the whole parsed form |
| `create_record`, `update_record` | the derived write | the create and update writes: [Writes](#writes) |
| `delete_record`, `bulk_delete_records` | delete by primary key | the delete writes |
| `before_commit(cx, change, ex)` | nothing | rows that commit with the write, such as an audit log: [Hooks](#hooks) |
| `after_commit(cx, change)` | nothing | side effects once the write commits: [Hooks](#hooks) |

## Naming

The names default from the type names, following Filament's conventions: `BlogPostResource`
over `BlogPost` serves `blog-posts`, titles a record "Blog post" and its list "Blog posts". The
plural rules are the `pluralizer` crate's, which Toasty names its tables with. Set `label` to
rename a record, and `plural_label` only when the rules guess wrong;
[`ResourceDef::slug`](https://y-l.fr/tablo/nightly/api/tablo/struct.ResourceDef.html#method.slug) and its siblings have the defaults.

## Scoping the query

`ResourceDef::query` narrows the rows every loader reads: the list, the export, the edit and
delete handlers, relationship options and the detail page. Use it for the resource's own row
scoping. Each panel mounts its own def, so one panel can narrow a resource that another serves
whole, such as a blog that serves the published posts only:

```rust
{{#include ../../../examples/guide/src/resources.rs:published-query}}
```

Three things do not belong in a `query` narrowing:

- **The tenant filter.** For a tenant-owned resource the framework adds it after the narrowings
  at every loader. See [Tenancy](./policy-auth-tenancy.md#tenancy).
- **Soft deletes.** Declare the column instead; see [Soft deletes](#soft-deletes).
- **Relations.** The list, the export and the detail page load the relations their columns
  declare: a `RelationColumn` or `CountColumn` declares its own, and a `ComputedColumn` declares
  one with `include`. Include a relation in `query` only when every loader
  reads it, for example because the policy does.

In your own code, load a resource's rows with
[`scoped_query::<R>(cx)?`](https://y-l.fr/tablo/nightly/api/tablo/fn.scoped_query.html), not `Query::all()`: it applies the
panel's narrowings, adds the tenant filter and leaves out the trashed records, and returns an
error rather than an unscoped query. A background job builds its context from the mounted panel's `PanelHandle`; see
[Outside a request](./data-access.md#outside-a-request).

The unique-value check on forms probes through the same scoped query, so a `#[unique]` index
wider than the scope is invisible to it: the check misses the collision and the database refuses
the write with a 500. Scope such indexes to match, as in `#[unique(tenant_id, email)]`.

## Soft deletes

A resource that keeps deleted records names the column a delete stamps, a nullable timestamp:

```rust
{{#include ../../../examples/guide/src/resources.rs:post-soft-delete}}
```

A delete then sets the column to the current time instead of removing the row, and every loader
leaves the stamped records out. The list's filter menu gains a **Trashed** control, and a trashed
record offers **Restore** and **Delete permanently**, which the policy allows through
`Ability::Restore` and `Ability::ForceDelete`. In your own code, `scoped_query_in::<R>(cx,
Trashed::Only)` loads the trashed records.
[`ResourceDef::soft_delete`](https://y-l.fr/tablo/nightly/api/tablo/struct.ResourceDef.html#method.soft_delete) has the rules, and
the hooks receive the writes as `Change::Restore` and `Change::ForceDelete`.

## Field access

A field only some users may see, or only some may change, declares who:

```rust
{{#include ../../../examples/guide/src/resources.rs:user-field-access}}
```

Each rule asks about the request, not the record, and it binds the field of its model: every
resource the panel mounts over that model obeys it. A user `view` refuses sees the field nowhere
the panel shows its records, and one `edit` refuses sees its control disabled.
[`ResourceDef::field_access`](https://y-l.fr/tablo/nightly/api/tablo/struct.ResourceDef.html#method.field_access) lists every place
the rule reaches.

The panel cannot see inside a closure, so a `ComputedColumn`, a `QueryFilter` and a `TextColumn`
with a `description` say which fields they read, and on a panel that restricts a field the mount
refuses one that does not:

```rust,ignore
ComputedColumn::new("Pay", |e: &Employee| e.salary.to_string())
    .reading(Reads::of(lens!(Employee.salary)))
```

A related record shows through the resource it names, which applies the restriction:
`RelationColumn::new::<R>` names the fields its closure reads, and `RelationColumn::of::<R>`
shows the record's title. A declaration that would show a restricted record the panel cannot mask,
such as `RelationColumn::unscoped`, fails to mount;
[`FieldAccessFault`](https://y-l.fr/tablo/nightly/api/tablo/enum.FieldAccessFault.html) lists each case.

## Writes

Every create, update and delete runs in a transaction the framework opens. For an update or a
delete, the handler first loads the target record through the scoped query inside that transaction
and checks policy on it. It then calls the resource's record function with the open transaction as
`ex`.

`create_record` and `update_record` default to writing the record form's fields
(`write_create` and `write_update`), so most resources declare neither. To check something inside
the transaction, override the function and delegate. This one refuses to move a comment onto a
post that is not published:

```rust
impl Resource for CommentResource {
    type Model = Comment;
    // …
{{#include ../../../examples/guide/src/resources.rs:comment-update-record}}
}
```

Run every statement through `ex`, and use the `record` you are given rather than loading it
again: it is the row the policy check passed. Both functions return the written row, or a
[`WriteError`](https://y-l.fr/tablo/nightly/api/tablo/enum.WriteError.html), which rolls the transaction back: `Refused` renders the
form again with each error under its field, and `Failed` answers as a failed write does.

A rule that reads the database refuses here rather than in `validate_record`, which runs before
the transaction opens and gets no executor: a stock level, a quota, or a uniqueness no index
models. A rule the write itself enforces, such as an update filtered on the stock it needs that
matches no row, holds against a concurrent write. The [CSV import](./tables.md#import) lists a
row its record fn refuses with the other refused rows, and rolls that row back to a savepoint, so
the rows after it are checked without its writes.

`delete_record` deletes the row by its primary key, or stamps it on a resource that soft-deletes,
and `bulk_delete_records` calls it once per record, so overriding `delete_record` covers both. A
bulk delete skips the records the policy refuses, as a bulk action does.

### Hooks

Every write the panel makes for a resource reaches two hooks with a `Change` that says what it
wrote: a create, an update, a delete or bulk delete, a restore or force delete, an
[action](./actions.md), or an attach or detach on a
[many-to-many relation](./detail-pages.md#many-to-many-relations). Neither runs when the write was
refused or failed.

`before_commit` runs inside the transaction, after the record function or the action wrote, with
the transaction as `ex`. A row written through `ex` commits with the write or not at all, so an
audit log belongs here:

```rust
{{#include ../../../examples/guide/src/resources.rs:audit-before-commit}}
```

An error rolls back the write and every row the hook wrote, and the request fails with the
write's failure toast. Run every statement through `ex`: `db(cx)` or `Notifications::send` opens a
second connection, which writes outside the transaction and, on a pool of one, waits forever.

`after_commit` runs once the transaction commits, before the response. Put work outside the
database there, such as email, webhooks and cache invalidation, so a rolled-back write never
triggers it:

```rust
{{#include ../../../examples/guide/src/resources.rs:notify-after-commit}}
```

An error it returns is logged and the write stays committed, and a crash between the commit and
the hook skips it, so a row that must exist whenever the write does is written in
`before_commit`.

[`Change`](https://y-l.fr/tablo/nightly/api/tablo/enum.Change.html) lists what each variant carries, and what a change does not
say, such as the links a form wrote. The audit example's match lists every variant without a `_`
arm, so a kind of write a later Tablo adds breaks the build instead of going unaudited.

A `Change` carries every column, including fields the user's [field access](#field-access) hides.
An audit table that lower-privilege users can read should store only what they may see.

The profile and tenant-settings pages save their own rows and reach neither hook.

## Startup checks

Mounting the panel builds each resource's def once and binds its table, form and detail page to
the database schema. It refuses a resource whose declaration cannot work, such as a `unique()`
field with no unique index, a column nothing writes on create, or a relation to a resource the
panel does not register, and lists every mistake it found in one `MountError`:
see [Mount errors](./mount-errors.md).

An action's `NAME` is checked when the app compiles: `ResourceDef::action` and
`ResourceDef::header_action` do not compile an action whose name is not one URL segment.

A modifier on the wrong kind of field does not compile: each `Field` constructor returns its
control's builder (`TextField`, `ChoiceField`, `FileField`, `CustomField`), which offers only
that control's modifiers.

A form control from elsewhere does not compile either: `ResourceDef::form` takes a
`Schema<R::Form>`, which places only the record form's own `controls()`, so a control of another
form, or a field built with `Field::text`, is a type error rather than a refused mount.

The panel serves the checked def to every request, so `declare()` never reads the request:
decisions about the user or the tenant belong to the policy and to the trait methods.
