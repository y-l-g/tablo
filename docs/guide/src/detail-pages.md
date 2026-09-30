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
link beside `Edit` on each row. A resource with no `view` declaration has no page and no link, and
the route answers 404 rather than rendering an empty shell.

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
- **Related rows** render through `view_relations(cx, record)`, the page's second half. The page
  loads through `view_query`, so include what the hook reads there:

```rust
fn view_query(cx: &Cx) -> Query<List<Post>> {
    let comments: Include<Post, List<Comment>> = Post::fields().comments().into();
    Self::query(cx).include(comments)
}
```

```rust
fn view_relations<'a>(cx: &'a Cx, record: &Post) -> Option<BoxView<'a>> {
    // The relation comes from `Resource::view_query`'s include, so this is the guard
    // the list columns use: drop the include and the page says so instead of
    // panicking inside `Deferred::get`.
    if record.comments.is_unloaded() {
        return Some(
            view! {
                cx =>
                <p class="text-sm text-destructive">
                    "Comments were not loaded by this query — add them to Resource::view_query's include."
                </p>
            }
            .boxed(),
        );
    }
    Some(render_relation::<CommentResource>(
        cx,
        "Comments",
        RelationColumns::columns(RelationColumn::computed("Comment", |c: &Comment| c.body.clone())),
        record.comments.get(),
    ))
}
```

It is a typed method and not a Schema field because a Schema renders the record's *string
projection* while a relation is a list of records — and `Resource::view(cx)` is handed no record at
all, so a Schema node could not read one. Reading `record.comments.get()` issues no query: it is the
row the include loaded, and a test counts the statements a detail page runs to hold that
(`the_relation_issues_no_query_of_its_own`). The call names the related `Resource`
(`render_relation::<CommentResource>`), so the table applies that resource's `can_view` to every
loaded row, and it renders at most `MAX_RELATION_ROWS` rows, printing a line that names the cap and
the total when it truncates. Related rows render read-only: no pager, no search, no bulk column, no
row actions. The table follows the list's fixed-layout rule: `table-fixed`, one `ColumnWidth` per
relation column (default `Wide`), `truncate` on every cell, and a `min-width` carrying one
readability floor per wide column, so long values clip to an ellipsis and a narrow viewport scrolls
instead of collapsing a column.
- A typed column (`Uuid`, `jiff::Timestamp`) is readable: bind it with `Field::text` (GH #192)
  and the view renders its stored value as text. A foreign key therefore reads as its stored id
  rather than the related record's label — render the relation through `view_relations` when the
  label is what a reader needs.
- `IntoSchema` takes at most eight top-level blocks; a longer view wraps a ninth in a `Group`.
