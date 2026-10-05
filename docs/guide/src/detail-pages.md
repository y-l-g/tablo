# Detail pages

A detail page shows one record, read-only, at `GET /admin/{slug}/{id}`. A resource declares it
with `ResourceDef::view`: a `Schema` built from the same fields and layout blocks as a form, which
the panel builds once when it mounts. The view defaults to the form, so a resource with a form has
a detail page showing the form's fields read-only. A form with fields that must stay off the
detail page sets a view with a subset or `Schema::empty()`. Set it to show other fields or another
layout:

```rust
impl Resource for PostResource {
    // …
    fn declare() -> ResourceDef<Self> {
        let c = PostForm::controls();
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:post-view}}
    }
}
```

A non-empty view adds a View action to each row. An empty one, `Schema::empty()`, turns the
detail page off: the route answers 404 and no row links to it. A `NoForm` resource has an empty
view unless it declares one.

## What the page shows

The header carries the record's title, a link back to the list, and an Edit link when the resource
has a form and the policy allows `Update` of this record. Below it come the view's fields, then any
[free-form content](#free-form-content), then the [related tables](#related-tables).

Each field renders its label and its stored value, never a control:

- a text field shows the value as text, typed values included;
- a choice shows the label of the matching option, or the stored value when none matches, so a
  relationship choice shows the stored key, not the related record's name;
- a file field shows the stored path as a link, under the rules in
  [File uploads](./forms.md#file-uploads);
- an embedded enum shows its variant's name and that variant's fields;
- layout blocks keep their structure.

**Where values come from.** A field the record form binds shows the form's value for it, the
same value the edit form starts with. `view_values(cx, record)` supplies every other key, as a map
from field name to display text; when both supply a key, the form's value wins. A `NoForm`
resource supplies every key there:

```rust
{{#include ../../../examples/guide/src/detail_pages.rs:detail-view-values}}
```

To show something that is not a column's own value, such as the author's name behind
`author_id`, render it as [free-form content](#free-form-content).

A field no source fills renders `(missing)`, and fails a `debug_assert!` in debug builds.

**Loading.** The record loads through `view_query` — `query` by default — with the tenant scope
applied. Include there every relation `view_values` or `view_content` reads:

```rust
impl Resource for PostResource {
    // …
{{#include ../../../examples/guide/src/resources.rs:post-view-query}}
}
```

An unknown id and an id outside the request's tenant are the same 404; a record the policy may not
`View` is a 403.

**Title.** `record_label` sets the page title; without it the title is the resource's `label()` and
the record's key, such as "Post 3f2a…":

```rust
{{#include ../../../examples/guide/src/detail_pages.rs:detail-record-label}}
```

`public_url(cx, record)` adds a "View public post" link to the header when it returns a URL.

## Free-form content

`view_content(cx, record)` renders anything that is not a field, such as a word count, below the
fields:

```rust
{{#include ../../../examples/guide/src/detail_pages.rs:detail-view-content}}
```

The returned view may borrow `cx` but not the record: compute what you need from the record first.

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

The foreign key's type is the owner's primary key type, or its `Option` for a nullable foreign
key; any other type does not compile. The table is
`CommentResource`'s — its columns, search, sort, filters and pager — over its tenant-scoped query
plus `post_id = <this post>`. It is titled with the related resource's plural label, or
`.label(..)`. The panel must register the related resource too: mounting refuses a relation to
one it does not.

- **Policy.** The related resource's policies apply as on its own list: no section renders when
  its policy refuses `ViewAny`, or when it is tenant-scoped and the request has no tenant, and each row keeps
  only the actions its record allows.
- **Detail page versus edit page.** On the detail page the table is read-only: rows keep only
  their View action. On the edit page rows also carry Edit and Delete, the table has bulk delete,
  and a create button appears when the related resource has a form and its policy allows `Create`.
- **Creating from the parent.** The create button opens the related resource's create page with
  the owner preselected, as in `/admin/comments/create?post_id=…`. The create page accepts such a
  parameter only for a relationship choice; the value is a default, and the submitted form is
  validated like any other create.
- **Returning.** Writes started from a related table carry `?return=` and redirect back to the
  page they started on. The panel follows `return` only to a path under its own prefix.
- **URL parameters.** Each related table's parameters are prefixed with the related resource's
  slug — `?comments.q=`, `?comments.sort=`, `?comments.after=` — so several tables share one page.
- **Live search.** A related table whose resource declares `live_search()` stays live: sorting,
  searching, filtering and paging re-render that table in place. Without JavaScript its links and
  forms fall back to full page loads.

Mounting the panel refuses a relation to a resource the panel does not register, and two relations of
one resource to the same related resource.
