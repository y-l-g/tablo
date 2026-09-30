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
filters combine with AND. At most 32 filters apply, each name and value at most 256 bytes.

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
- **Delete** when `can_delete_any` allows deletes and `can_view` and `can_delete` allow the record.

A row that allows none keeps an empty actions cell.

When `can_delete_any` allows deletes, the list adds a checkbox column and a bulk bar. A row whose
record may not be deleted gets no checkbox, so select-all only selects deletable rows. A bulk
delete accepts at most 400 records and deletes all of them or none.

Both deletes ask first. The Delete action opens a confirmation dialog on the list page;
confirming it deletes the row, shows a notification and refreshes the table without leaving the
page. The bulk bar's button opens a dialog stating how many rows are selected. The delete
handlers refuse a POST that was not confirmed through the dialog with 400, and without
JavaScript the Delete link renders the list with its dialog already open.

If the table fails to load, the list shows an error state with a retry link in place of the rows;
the rest of the page still renders.
