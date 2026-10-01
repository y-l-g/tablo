# Tables

A resource's `table()` declares its list page: the columns, the row key, and the search, sort,
filter, grouping and pagination the list offers. The same declaration drives the CSV export.

```rust
fn table(_cx: &Cx) -> Table<User> {
    Table::new(
        |u: &User| u.id.to_string(),
        (
            TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone())
                .searchable()
                .sortable(),
            TextColumn::r#for(User::fields().email(), |u: &User| u.email.clone()).searchable(),
            TextColumn::computed("Status", |u: &User| {
                if u.active { "Active" } else { "Inactive" }.to_string()
            }),
        ),
    )
    .paginate(20)
}
```

## The row key

`Table::new(key, columns)` takes the row key first. It must be the model's primary key as a
string: the table uses it to identify rows for selection and in-place updates, and the action
URLs and bulk checkboxes carry it, where handlers parse it back into the primary key. A key that
is not the primary key makes every delete answer 404.

When rows must be keyed by something else in the page, `Table::new_split(display, record,
columns)` takes the two keys separately: `display` identifies rows in the page and `record` is the
primary key the URLs carry. Either way, keys must be unique within a page.

## Columns

| Constructor | Cell | Search and sort |
| --- | --- | --- |
| `TextColumn::r#for(lens, project)` | `project(row)`, bound to a `String` field | `.searchable()`, `.sortable()` |
| `TextColumn::computed(label, project)` | `project(row)` | not available: calling either panics |
| `BooleanColumn::r#for(lens, project)` | a check or a cross icon for a `bool` field; the export writes `Yes`/`No` (`.labels(..)`) | `.sortable()` |

- **Labels.** A field column is labelled from its field name (`created_at` → "Created at"); a
  computed column uses the label you pass.
- **Relations.** A column whose closure reads a relation declares it with `.include(..)`, and the
  list and the export load it with the page's rows in one query. A relation no column includes
  is not loaded. Guard the read so a missing include fails loudly instead of showing blank data:

  ```rust
  TextColumn::computed("Author", |p: &Post| {
      if p.author.is_unloaded() { "(unloaded)".into() } else { p.author.get().name.clone() }
  })
  .include(Post::fields().author())
  ```

- **Widths.** The table uses a fixed layout: a column's width is what it declares, not the width of
  its widest cell, so paging and filtering never shift the columns. A field column takes an equal
  share of the space left over; a computed column defaults to a narrow share of the table (10%,
  scaled down when many columns claim one). Override with `.width(ColumnWidth::Percent(30))`,
  `Rem(8)`, `Narrow` or `Wide`. A cell wider than its column is truncated with an ellipsis. On a
  narrow screen the table keeps a minimum width and scrolls horizontally instead of crushing its
  columns.

Two columns with the same name, or a table with no columns, panic; `Panel::build` reports it as a
startup error.

### Your own columns

A column is anything that implements `Column<M>`. `TextColumn` and `BooleanColumn` implement it and
nothing more, so a column of your own reaches as far as theirs:

```rust
struct Initials;

impl Column<User> for Initials {
    fn name(&self) -> &str { "initials" }
    fn label(&self) -> &str { "Initials" }

    // The export's cell, and the table's unless `cell` renders a view.
    fn text(&self, u: &User) -> String {
        u.name.split_whitespace().filter_map(|w| w.chars().next()).collect()
    }

    fn cell<'a>(&self, cx: &'a Cx, u: &User) -> BoxView<'a> {
        let text = self.text(u);
        view! { cx => <span class="font-mono">(text)</span> }.boxed()
    }
}
```

Only `name`, `label` and `text` are required. The other methods default to a column that is
narrow, not searchable, not sortable and reads no relation: override `column_width`,
`is_searchable` and `search_expr`, `is_sortable` and `order_by`, or `includes` to change that. Put
the column in the tuple next to the built-in ones, or append it with `Table::column(..)`, which
also takes columns past the tuple's eight.

## Search, sort and pagination

The URL holds the list's whole state, so every view of a list is a link you can share:

| Parameter | Effect |
| --- | --- |
| `?q=ada` | search: each searchable column contains `ada`; any column may match |
| `?sort=name&dir=desc` | sort by a sortable column; `dir` is `asc` (default) or `desc` |
| `?after=…`, `?before=…` | the next or previous page, as an opaque cursor |
| `?f.status=published` | a filter: see [Filters](#filters) |
| `?group_by=status` | grouping: see [Grouping](#grouping) |

- Search escapes `%` and `_`, so they match literally. Terms are trimmed and capped at 128
  characters. Matching follows the database's `LIKE`: case-insensitive for ASCII on SQLite,
  case-sensitive on PostgreSQL.
- Pagination is cursor-based, 25 rows per page unless `.paginate(n)` sets another size. The
  primary key breaks ties, so a sort over duplicate values still pages deterministically.
- All of it works without JavaScript. `.hide_search()` removes the search box, and
  `.hide_filter_bar()` the filter controls.

### Live updates

```rust
Table::new(|u: &User| u.id.to_string(), columns).live_search()
```

With `live_search()`, typing in the search box, sorting, filtering and paging update the table
in place, without a page load, keeping focus and scroll position. The plain links and forms
remain for visitors without JavaScript.

## Filters

```rust
.filters((
    SelectFilter::r#for(Post::fields().status(), vec!["draft".into(), "published".into()]),
    TernaryFilter::r#for(Post::fields().featured()),
    DateFilter::r#for(Post::fields().created_at()),
))
```

| Filter | Field | Values |
| --- | --- | --- |
| `SelectFilter::r#for(lens, options)` | `String` | one of `options`, matched exactly |
| `TernaryFilter::r#for(lens)` | `bool` | `true`, `false`, or `all` (no filter) |
| `DateFilter::r#for(lens)` | `jiff::Timestamp` | a date `2024-01-15` matches that UTC day; an RFC 3339 timestamp matches that instant |
| `VariantFilter::r#for(name, label, options)` | any | named options, each a Toasty predicate you build |

Each active filter is one parameter, `?f.<name>=<value>`, named after the field, and active
filters combine with AND.

A filter is anything that implements `Filter<M>`: a name, a label, the predicate a value selects,
and the control the filter bar renders. The four filters above implement it and nothing more.
`FilterInput` carries the parameter the control submits and the current value, and
`input.select(cx, options)` renders the built-in select:

```rust
struct Adults;

impl Filter<User> for Adults {
    fn name(&self) -> &str { "adults" }
    fn label(&self) -> &str { "Adults" }
    fn to_expr(&self, value: &str) -> Option<Expr<bool>> {
        (value == "yes").then(|| User::fields().age().ge(18))
    }
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        input.select(cx, vec![("yes".into(), "Adults only".into())])
    }
}
```

A filter of your own goes in the `filters((..))` tuple; alone, it is a one-element tuple,
`.filters((Adults,))`. At most 32 filters apply, each name and value at most 256 bytes.

A filter that cannot apply — an unknown name, a value the filter rejects, or one over the limits —
is never dropped silently: the list shows a warning banner naming it, and the export refuses the
request with 400 rather than export more rows than asked.

## Grouping

```rust
.group_by("status", |p: &Post| p.status.clone())
```

`?group_by=status` groups the current page's rows under headers with a row count. Grouping runs
on the loaded page, so the counts cover that page, not the whole table. A `?group_by=` value the
table does not declare is ignored.

## Export

`GET /admin/{slug}/export` returns the list as a CSV file named `{slug}.csv`, with the current
search, filters and sort applied, and the relations the columns include loaded.

- Rows the caller may not view (`can_view`) are left out.
- The export delivers at most 10,000 rows. When the search and filters match more, it answers
  413 rather than a truncated file.
- Cells that a spreadsheet would read as a formula are escaped. Add `?bom=1` to prefix the file
  with a byte-order mark for Excel.

## Row actions and deletes

Each row shows the actions its record allows:

- **View** when the resource declares a detail page (`view()`) and `can_view` allows the record;
- **Edit** when the resource has a record form and `can_view` and `can_update` allow it;
- **Delete** when `can_delete_any` allows deletes and `can_view` and `can_delete` allow the record;
- each [custom action](#custom-actions) the record allows.

A row that allows none keeps an empty actions cell.

When `can_delete_any` allows deletes, or the resource declares a bulk custom action, the list adds
a checkbox column and a bulk bar. A row that neither delete nor any bulk action allows gets no
checkbox, so select-all only selects rows something can be done to. A bulk delete accepts at most
400 records and deletes all of them or none: a selection holding a record that may not be deleted
deletes nothing and returns to the list with an error notification.

Both deletes ask first. The Delete action opens a confirmation dialog on the list page;
confirming it deletes the row, shows a notification and refreshes the table without leaving the
page. The bulk bar's button opens a dialog stating how many rows are selected. The delete
handlers refuse a POST that was not confirmed through the dialog with 400, and without
JavaScript the Delete link renders the list with its dialog already open.

### Custom actions

An action is a mutation beyond create, update and delete, declared as a type implementing
`Action<R>` and listed by `Resource::actions`:

```rust
struct Publish;

impl Action<PostResource> for Publish {
    const NAME: &'static str = "publish";

    fn label() -> String {
        "Publish".to_string()
    }

    fn can_run(_cx: &Cx, post: &Post) -> bool {
        post.status != "published"
    }

    async fn run(_cx: &Cx, posts: &[Post], ex: &mut dyn toasty::Executor) -> Result<()> {
        for post in posts {
            Post::filter(Post::fields().id().eq(post.id))
                .update()
                .status("published".to_string())
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}

impl Resource for PostResource {
    fn actions() -> Actions<Self> {
        Actions::new().add::<Publish>()
    }
    // …
}
```

A row renders the action's button when `can_view` and `can_run` allow its record, and the bulk bar
renders it for the selection. `const ROW: bool = false` keeps it off the rows, and
`const BULK: bool = false` off the bulk bar.

The framework runs an action the way it runs a delete. The POST goes to
`{list}/{key}/actions/{NAME}` for a row and `{list}/actions/{NAME}` for the selection, carries the
CSRF token, and loads the records through the tenant-scoped query inside a transaction. Every
record must pass `can_view` and `can_run`, and `run` writes through the same transaction, so an
error rolls everything back. After the commit, `after_commit` receives `Mutation::Action(NAME)`
with the records and the list shows `Action::success`, by default the label and the record count.

A row the action refuses answers 403. A selection that holds one writes nothing and returns to the
list with an error notification. `Panel::build` refuses an action name that is not one URL
segment, or that two actions of a resource share. Custom actions run without a confirmation
dialog.

If the table fails to load, the list shows an error state with a retry link in place of the rows;
the rest of the page still renders.
