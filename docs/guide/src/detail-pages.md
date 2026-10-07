# Detail pages

A detail page shows one record, read-only, at `GET /admin/{slug}/{id}`. A resource declares it
with `ResourceDef::view`: a `Detail` of the same columns a [table](./tables.md) lists, arranged in
`Section`, `Group` and `Grid` blocks, which the panel builds once when it mounts. Each column reads
its value off the typed record, and declares the relations it reads:

```rust
impl Resource for PostResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:post-view}}
    }
}
```

A non-empty view adds a View action to each row. An empty one, `Detail::empty()`, turns the detail
page off: the route answers 404 and no row links to it.

## The default view

Without `view`, the record form derives the detail page, as it derives the table: one column per
field, in declaration order.

- a text field shows its value, typed values included, as the edit form spells them;
- an options field shows its option's label;
- a `bool` shows as yes or no;
- a `#[form(file)]` field shows the stored path as a link, under the rules in
  [File uploads](./forms.md#file-uploads);
- an `#[form(embed)]` value shows each leaf under its own label, and an embedded enum shows its
  variant's name and that variant's leaves only.

A bare `#[form(choice)]` holds a key, such as `author_id`, so it shows nothing: the related
record shows through a column that includes the relation, as `Author` does above. A `NoForm`
resource derives no column, so it has no detail page unless it declares one:

```rust
{{#include ../../../examples/guide/src/detail_pages.rs:detail-no-form}}
```

## What the page shows

The header carries the record's title, a link back to the list, and an Edit link when the resource
has a form and the policy allows `Update` of this record. Below it come the view's columns, then
the [related tables](#related-tables).

Each column renders its label over its cell, never a control. The built-in columns render as in a
table: `TextColumn` the value, or what its `format` returns, `BooleanColumn` an icon labelled yes
or no, `FileColumn` a link, and `ComputedColumn` the text its closure returns. Layout blocks keep
their structure.

A `ComputedColumn` shows anything the record determines, such as a reading time:

```rust
{{#include ../../../examples/guide/src/detail_pages.rs:detail-computed}}
```

An app's own [`Column`](./tables.md) renders its `cell` on the detail page too. A column that
shows more than one value under one label overrides `entry`, which renders the label over the cell
by default; `EmbeddedColumn` overrides it to give each leaf its own label.

**Loading.** The record loads through the resource's tenant-scoped `query`, plus every relation the
view's columns declare with `include`, each once, wherever its block sits. No other relation
loads: a `ComputedColumn` reading one it does not declare finds it unloaded.

An unknown id and an id outside the request's tenant are the same 404; a record the policy may not
`View` is a 403.

**Title.** `record_label` sets the page title; without it the title is the resource's `label()` and
the record's key, such as "Post 3f2a…":

```rust
{{#include ../../../examples/guide/src/resources.rs:post-record-label}}
```

**Public link.** `public_link` adds a link to the record's public page to the header of its detail
and edit pages, for each record it returns one for: the URL to link and the text to show.

```rust
{{#include ../../../examples/guide/src/resources.rs:post-public-link}}
```

## Related tables

`ResourceDef::relation` adds a related resource whose rows belong to a record. Each renders on
the record's detail page as that resource's own list table, narrowed to the record:

```rust
impl Resource for PostResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:post-relations}}
    }
}
```

The foreign key's type is the owner's primary key type, or its `Option` for a nullable foreign
key; any other type does not compile. The table is
`CommentResource`'s — its columns, search, sort, filters and pager — over its tenant-scoped query
plus `post_id = <this post>`. It is titled with the related resource's plural label, or
`.label(..)`. The panel must register the related resource too: mounting refuses a relation to
one it does not.

- **Policy.** The related resource's policies apply as on its own list: no section renders when
  its policy refuses `ViewAny`, or when it is tenant-scoped and the request has no tenant, and each row keeps
  only the actions its record allows.
- **Actions.** Rows carry the related resource's View, Edit, Delete and custom actions, the table
  has its bulk bar, and a create button appears when the related resource has a form and its
  policy allows `Create`. The edit page renders no related table: a change to one reruns the page,
  which would reset the form fields not yet saved.
- **Creating from the parent.** The create button opens the related resource's create page with
  the owner preselected, as in `/admin/comments/create?post_id=…`. The create page accepts such a
  parameter only for a relationship choice; the value is a default, and the submitted form is
  validated like any other create.
- **Returning.** Writes started from a related table carry `?return=` and redirect back to the
  page they started on. The panel follows `return` only to a path under its own prefix.
- **URL parameters.** Each related table's parameters are prefixed with the related resource's
  slug — `?comments.q=`, `?comments.sort=`, `?comments.after=` — so several tables share one page.
- **Live updates.** Sorting, searching, filtering and paging update the table in place, like a
  list's. Without JavaScript its links and its search form load the page.

Mounting the panel refuses a relation to a resource the panel does not register, and two relations of
one resource to the same related resource.
