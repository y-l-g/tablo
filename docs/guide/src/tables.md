# Tables

A resource's `Table`, set with `ResourceDef::table`, declares its list page: the columns, and
the search, sort, filter, grouping and pagination the list offers. It depends on no request: the
panel builds it once when it mounts and serves that table to every request. The same declaration
drives the CSV export.

The table defaults to the record form's derived table, `UserForm::table()`: a sortable column per
text field, searchable over a `String` or `Option<String>`, an `#[form(options = ..)]` field by
its option's label, and a `bool` as yes or no. A bare choice, a file and an embedded value get no
column. Extend the derived table, or declare the columns yourself:

```rust
ResourceDef::new().table(UserForm::table().filters(TernaryFilter::new(User::fields().active())))
```

```rust
impl Resource for UserResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
{{#include ../../../examples/guide/src/resources.rs:user-table}}
    }
}
```

Each row is keyed by its record's primary key: the table identifies rows for selection and
in-place updates by it, and the action URLs and bulk checkboxes carry it. A composite primary key
has no URL form, so its rows render without row actions or bulk selection.

## Lenses

`lens!(User.name)` names a field once and yields both halves a column needs: the path a query
sorts and searches on (`User::fields().name()`) and the reader that renders the loaded value
(`&user.name`). The two cannot disagree. A lens names a field of the model or, through an embedded
value, its leaf (`lens!(Post.seo.title)`), which binds the flattened `seo_title` column; a lens
through a relation does not compile, since a relation's records are not part of the row. Builders
that only query, such as the filters and `Field` constructors, take either a lens or a plain path.

## Columns

| Constructor | Cell | Search and sort |
| --- | --- | --- |
| `TextColumn::new(lens)` | the field's value as text, or `.format(\|value\| ..)` of it | `.searchable()` on a `String` field, `.sortable()` |
| `ComputedColumn::new(label, project)` | `project(row)` | neither: the methods do not exist |
| `BooleanColumn::new(lens)` | a check or a cross icon for a `bool` field; the export writes `Yes`/`No` (`.labels(..)`) | `.sortable()` |

```rust
{{#include ../../../examples/guide/src/tables.rs:table-format}}
```

- **Labels.** A field column is labelled from its field name (`created_at` → "Created at"); a
  computed column uses the label you pass.
- **Relations.** A computed column whose closure reads a relation declares it with
  `.include(..)`, and the list and the export load it with the page's rows in one query. A
  relation no column includes is not loaded. Guard the read so a missing include renders
  `(unloaded)` instead of blank data:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-relation-column}}
```

- **Widths.** The table uses a fixed layout: a column's width is what it declares, not the width of
  its widest cell, so paging and filtering never shift the columns. A field column takes an equal
  share of the space left over; a computed column defaults to a narrow share of the table (10%,
  scaled down when many columns claim one). Override with `.width(ColumnWidth::Percent(30))`,
  `Rem(8)`, `Narrow` or `Wide`. A cell wider than its column is truncated with an ellipsis. On a
  narrow screen the table keeps a minimum width and scrolls horizontally instead of narrowing its
  columns.

Two columns with the same name, two filters with the same name, a table with no columns, and a
zero page size are misdeclarations: mounting the panel refuses the resource, and rendering the table
fails with the same errors.

### Your own columns

A column is anything that implements `Column<M>`. The built-in columns implement it and nothing
more, so a column of your own reaches as far as theirs:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-custom-column}}
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
{{#include ../../../examples/guide/src/tables.rs:table-live-search}}
```

With `live_search()`, typing in the search box, sorting, filtering and paging update the table
in place, without a page load, keeping focus and scroll position. The plain links and forms
remain for visitors without JavaScript.

## Filters

```rust
{{#include ../../../examples/guide/src/tables.rs:table-filters}}
```

| Filter | Field | Values |
| --- | --- | --- |
| `SelectFilter::new(lens, options)` | `String` | one of `options`, matched exactly; the options are `Vec<(String, String)>` (an [`Options`](./forms.md#controls) list), `Vec<String>`, or `[&str; N]` |
| `TernaryFilter::new(lens)` | `bool` | `true`, `false`, or `all` (no filter) |
| `DateFilter::new(lens)` | `jiff::Timestamp` | a date `2024-01-15` matches that UTC day; an RFC 3339 timestamp matches that instant |
| `QueryFilter::new(name, label).option(label, predicate)` | any | named options, each a Toasty predicate you build |

Each active filter is one parameter, `?f.<name>=<value>`, named after the field (a
`QueryFilter` after its name), and active
filters combine with AND.

A filter is anything that implements `Filter<M>`: a name, a label, the predicate a value selects,
and the control the filter bar renders. The four filters above implement it and nothing more.

```rust
{{#include ../../../examples/guide/src/tables.rs:table-promoted-filter}}
```

`FilterInput` carries the parameter the control submits and the current value, and
`input.select(cx, options)` renders the built-in select:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-adults-filter}}
```

A filter of your own goes in the `filters((..))` tuple; alone, it is a one-element tuple,
`.filters((Adults,))`. At most 32 filters apply, each name and value at most 256 bytes.

A filter that cannot apply — an unknown name, a value the filter rejects, or one over the limits —
is never dropped silently: the list shows a warning banner naming it, and the export refuses the
request with 400 rather than export more rows than asked.

## Grouping

```rust
{{#include ../../../examples/guide/src/tables.rs:table-group-by}}
```

`?group_by=status`, named after the field, groups the current page's rows under headers with a
row count. Grouping runs on the loaded page, so the counts cover that page, not the whole table. A
`?group_by=` value the table does not declare is ignored.

## Export

`GET /admin/{slug}/export` returns the list as a CSV file named `{slug}.csv`, with the current
search, filters and sort applied, and the relations the columns include loaded.

- Rows the policy may not `View` are left out.
- The export delivers at most 10,000 rows. When the search and filters match more, it answers
  413 rather than a truncated file.
- Cells that a spreadsheet would read as a formula are escaped. Add `?bom=1` to prefix the file
  with a byte-order mark for Excel.

## Row actions and deletes

Each row shows the actions its record allows:

- **View** when the resource declares a detail page (a non-empty view) and the policy allows `View` of the
  record;
- **Edit** when the resource has a record form and the policy allows `View` and `Update`;
- **Delete** when the policy allows `DeleteAny`, and `View` and `Delete` of the record;
- each [custom action](#custom-actions) the record allows.

A row that allows none keeps an empty actions cell.

When the policy allows `DeleteAny`, or the resource declares a bulk custom action, the list adds
a checkbox column and a bulk bar. A row that neither delete nor any bulk action allows gets no
checkbox, so select-all only selects rows something can be done to. A bulk delete accepts at most
400 records and deletes all of them or none: a selection holding a record that may not be deleted
deletes nothing and returns to the list with an error notification.

Both deletes require confirmation. The Delete action opens a confirmation dialog on the list page;
confirming it deletes the row, shows a notification and refreshes the table without leaving the
page. The bulk bar's button opens a dialog stating how many rows are selected. The delete
handlers refuse a POST that was not confirmed through the dialog with 400, and without
JavaScript the Delete link renders the list with its dialog already open.

### Custom actions

An action is a mutation beyond create, update and delete, declared as a type implementing
`Action<R>` and added to the def with `ResourceDef::action`:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-publish-action}}

impl Resource for PostResource {
    // …
    fn declare() -> ResourceDef<Self> {
        ResourceDef::new()
            // …
{{#include ../../../examples/guide/src/resources.rs:post-actions}}
    }
}
```

A row renders the action's button when the policy's `View` and the action's `can_run` allow its
record, and the bulk bar
renders it for the selection. `const ROW: bool = false` keeps it off the rows, and
`const BULK: bool = false` off the bulk bar.

The framework runs an action the way it runs a delete. The POST goes to
`{list}/{key}/-/actions/{NAME}` for a row and `{list}/-/actions/{NAME}` for the selection, carries the
CSRF token, and loads the records through the tenant-scoped query inside a transaction. Every
record must pass the policy's `View`, and `run` writes through the same transaction, so an error
rolls everything back. After the commit, `after_commit` receives `Mutation::Action(NAME)` with the
records and the list shows `Action::success`, by default the label and the record count.

A record `can_run` refuses is not handed to `run`: a refused row answers 403, and a selection drops
the refused records, runs the rest and appends the refused count to `Action::success`
(`"Publish: 3 records (2 skipped)"`). A selection every record refuses writes nothing and returns
to the list with an error notification. A record that fails the policy's `View`, and one the scoped
query no longer returns, fail the whole POST instead: 403 and 404, and nothing is written. An
action name that is not one URL segment does not compile,
and mounting the panel refuses a name two actions of a resource share. A destructive action
declares `const CONFIRM: bool = true` to ask first through a confirmation dialog carrying the
delete dialog's mechanism and wording; an unconfirmed POST answers 400. Confirmatory buttons need
JavaScript: without it they do nothing.

If the table fails to load, the list shows an error state with a retry link in place of the rows;
the rest of the page still renders.
