# Detail pages

A read-only page for one record (GH #187): the `view` schema, what renders, related rows, and the
current limits.

A resource can show one record read-only by declaring `view`, a `Schema` in the form's vocabulary
read the other way round (ADR-0016). Like Filament's infolist it is its own declaration: it may show
keys the form does not, and a resource with no form declares one too.

```rust
fn view(cx: &Cx) -> Schema {
    Schema::new(Section::new("Post").schema((
        Field::text(Post::fields().title()),
        Field::text(Post::fields().body()).multiline(6),
    )))
}
```

That registers `GET /admin/{slug}/{id}` — loaded through the tenant-scoped query, so an unknown id
and one outside the tenant are the same 404, while `can_view` denial is a 403 — and adds a `View`
control beside `Edit` on each row. A resource with no `view` declaration has no page and no link, and
the route answers 404 rather than rendering an empty shell. The page's header links back to the
list, and to the edit form when the resource has one and `can_update` allows this record — the gate
the row's `Edit` control uses.

The heading is the record's label when the resource declares one:

```rust
fn record_label(cx: &Cx, record: &Post) -> Option<String> {
    Some(record.title.clone())
}
```

The default returns `None`, and the heading is then `{label} {id}` — one record's name and the URL's
record key. A label is display text, not a key: two records may share one, so it does not
replace `Table::new`'s key, which must stay injective within a page for keyed diffs (GH #241).

- **Read-only is not a disabled form.** Fields render labels and stored values:
  a text field shows text, a choice shows the option label the form offered (or the stored value
  when no option matches, a relationship key included), a file field shows the stored path as a link
  to the file (GH #242), an embedded enum shows its stored variant's name and payload, and layout
  blocks keep their structure. No control, no CSRF field, no validation slot.
- **Values come from the form's projection** (`RecordForm::hydrate`), so a field that renders in
  the form renders here; `view_values` adds a key only the view shows. A `NoForm` resource
  supplies every key through `view_values`. A field whose key neither supplies renders `(missing)`
  and fails a `debug_assert!`, as a list column shows `(unloaded)` for a relation its query did not
  load (ADR-0011): a blank would read as an empty value.
- **Free-form content** — anything read off the record that is not one of its fields, such as a
  word count — renders through `view_content(cx, record)`, below the fields. The page loads
  through `view_query`, so include there any relation the hook or `view_values` reads.
- **Related rows** render as [relations](#relations), below the content.
- A typed column (`Uuid`, `jiff::Timestamp`) is readable: bind it with `Field::text` (GH #192)
  and the view renders its stored value as text. A foreign key therefore reads as its stored id
  rather than the related record's label — show the label through `view_values` when a reader
  needs it.
- `IntoSchema` takes at most eight top-level blocks; a longer view wraps a ninth in a `Group`.

## Relations

A record's related rows — a post's comments — render on its detail and edit pages as the related
resource's own list table, narrowed to the record (Filament's relation managers):

```rust
impl Resource for PostResource {
    fn relations() -> Vec<Relation<Post>> {
        vec![Relation::has_many::<CommentResource, _>(
            Comment::fields().post_id(),
            |post: &Post| post.id,
        )]
    }
}
```

`has_many` names the related resource and the binding between the two: the related model's
foreign-key column and the owner's value for it (a nullable key takes the owner's value in
`Some`). The table is `CommentResource`'s: its columns, search, sort, filters and pager, over its
tenant-scoped query plus `post_id = <this post>`. Its policies apply as on its list: a request
`can_view_any` refuses, or one without a tenant the related resource requires, gets no section.
The section is titled with the related resource's navigation label; `.label(..)` overrides it.

The detail page shows the rows read-only, as Filament's view page does: each row keeps its View
link. The edit page carries the writes: the row Edit and Delete actions, bulk delete, and the
create link, each gated per row or by `can_create` as on the list.

The relation's URL parameters are prefixed with the related resource's slug — `?comments.q=`,
`?comments.sort=`, `?comments.after=` — so several relations share one page without colliding.
A link or a form in one relation carries only that relation's parameters.

On the edit page, when the related resource has a form and `can_create` allows it, the section
links a create button to that resource's create page with the owner already chosen
(`/admin/comments/create?post_id=…`): the create page seeds a relationship control a query
parameter names, and nothing else. The seed is a prefill, and the write checks what is submitted
as for any create.

Writes started from the relation return to the page it is on: the create link, the row edit and
delete actions and the bulk delete carry `?return=`, and the write redirects there instead of to
the related resource's list. The panel follows a `return` only to a path under its own prefix.

`Panel::build` rejects a relation to a resource the panel does not register — its row actions and
create link would lead nowhere — and two relations of one resource to the same related resource.
A table declared `live_search` stays live in a relation: sorting, searching, filtering and paging
re-render that table in place, with its URL parameters prefixed by the relation's key, and without
JavaScript its links and forms fall back to full page loads.
