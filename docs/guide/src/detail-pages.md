# Detail pages

A detail page shows one record, read-only, at `GET /admin/{slug}/{id}`. A resource declares it
with `ResourceDef::detail`: a `Detail` of the same columns a [table](./tables.md) lists, arranged in
`Section`, `Group`, `Grid` and `Tabs` blocks, which the panel builds once when it mounts. Each column reads
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

A non-empty detail adds a View action to each row, and `Detail::empty()` turns the page off.
[`ResourceDef::detail`](https://y-l.fr/tablo/nightly/api/tablo/struct.ResourceDef.html#method.detail) says what the page's header
carries, what it loads and what it answers for a record the request may not see.

## The default detail page

Without `detail`, the model's [`ModelColumns`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.ModelColumns.html) give the detail
page, as they give the table: one column per field the table lists, plus its embedded values and
lists, in declaration order. It reads the model, not the record form, so a field Toasty writes,
such as a `#[default]` timestamp, shows too, and a `NoForm` resource has a detail page as well.
The derive's page lists the column each type gets and the fields it leaves out.

A relation shows only through a column that includes it, as `Author` does above, and a file path
shows as text until a `FileColumn` links it. A resource declares its own detail page to pick or
arrange the columns:

```rust
{{#include ../../../examples/guide/src/detail_pages.rs:detail-no-form}}
```

## What the page shows

The header carries the record's title, its [actions](./actions.md#where-it-shows), Delete and
Edit, each as the policy allows them on this record. Below it come the detail's columns, then the
[related tables](#related-tables).

Each column renders its label over its cell, as it renders in a table, never a control. A
`ComputedColumn` shows anything the record determines, such as a reading time:

```rust
{{#include ../../../examples/guide/src/detail_pages.rs:detail-computed}}
```

An app's own [`Column`](./tables.md) renders its `cell` on the detail page too; `Detail::column`
appends one, and an `IntoDetail` impl for its type places it in a block. A column that shows more
than one value under one label overrides `entry`, which renders the label over the cell by
default; `EmbeddedColumn` overrides it to give each leaf its own label.

**Loading.** The page loads only the relations its columns declare: a relation column declares its
own, and a `ComputedColumn` declares one with `include`. A `ComputedColumn` reading one it does
not declare finds it unloaded. A page of the app's own renders a `Detail` the same way, with
[`Detail::load`](https://y-l.fr/tablo/nightly/api/tablo/struct.Detail.html#method.load) and `render`.

**Title.** `record_title` names the text column that titles each record, here and wherever another
resource points at it: each option of a relationship choice over the resource, and each cell of a
`RelationColumn::of` it. A title built from several columns is a column the model stores;
[`ResourceDef::record_title`](https://y-l.fr/tablo/nightly/api/tablo/struct.ResourceDef.html#method.record_title) has the fallback
and the rules:

```rust
{{#include ../../../examples/guide/src/resources.rs:post-record-title}}
```

**Public link.** `public_link` adds a link to the record's public page to the header of its detail
and edit pages, for each record it returns one for: the URL to link and the text to show.

```rust
{{#include ../../../examples/guide/src/resources.rs:post-public-link}}
```

## Related tables

`ResourceDef::relation` adds a related resource whose rows belong to a record. Each renders on
the record's detail and edit pages as that resource's own list table, narrowed to the record:

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

The table is `CommentResource`'s own list table, its columns, search, sort, filters and pager
included, narrowed to the post: the related resource keeps its policy, its actions and its create
button, which opens with the post preselected. The panel must register the related resource too.
[`Relation`](https://y-l.fr/tablo/nightly/api/tablo/struct.Relation.html) lists what the table carries, where its writes land, and
how several tables share one page.

### Many-to-many relations

`Relation::belongs_to_many` names the owner's `#[has_many(via = ..)]` field over a join model,
declared as for a [many-to-many field](./forms.md#many-to-many-fields). The table lists the
related records the owner links, and attaches and detaches them as join rows:

```rust
impl Resource for TagResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/forms.rs:tag-relations}}
    }
}
```

The section's header carries Attach, and each row and the bulk bar carry Detach, which deletes
the join rows and leaves the records. Both ask the owner's policy for `Update`, since linking a
record edits the owner. The rows keep the related resource's View, Edit and Delete, but no custom
action and no create button: a record created there would link nothing.
[`Relation::belongs_to_many`](https://y-l.fr/tablo/nightly/api/tablo/struct.Relation.html#method.belongs_to_many) has the routes, the
hooks and the mount checks.
