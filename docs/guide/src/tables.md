# Tables

A resource's `Table`, set with `ResourceDef::table`, declares its list page: the columns, and
the search, sort, filter, grouping and pagination the list offers. It depends on no request: the
panel builds it once when it mounts and serves that table to every request. The same declaration
drives the CSV export.

The table defaults to the record form's derived table, `UserForm::table()`: a sortable column per
text field, searchable over a `String` or `Option<String>`, an `#[form(options)]` field by its
option's label, and a `bool` as yes or no. A relationship, a file and an embedded value get no
column. Extend the derived table, or declare the columns yourself:

```rust
ResourceDef::new().table(UserForm::table().filters(TernaryFilter::new(lens!(User.active))))
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
through a relation does not compile, since a relation's records are not part of the row.
`relation!(Post.author)` is a relation's lens: the include that loads it and the reader of the
loaded records, which the relation columns take. Builders that only query, such as the filters,
`Tenancy::column` and the `Field` constructors, take either a lens or a plain path; this guide
names a field with `lens!` wherever one fits, and writes `User::fields()` only where a lens
cannot go: a query expression, a path through a relation such as `Tenancy::via`'s, and the
foreign key `Relation::has_many` takes.

## Columns

| Constructor | Cell | Search and sort |
| --- | --- | --- |
| `TextColumn::new(lens)` | the field's value as text (an `Options` enum's label), or `.format(\|value\| ..)` of it | `.searchable()` on a `String` field, `.sortable()` |
| `ComputedColumn::new(label, project)` | `project(row)` | neither: the methods do not exist |
| `RelationColumn::of::<R>(relation!(..))` | a `Deferred` `belongs_to` or `has_one` field's record, titled as `R` titles it: a resource's `record_label`; a dash for a nullable field holding none | neither |
| `RelationColumn::new(relation!(..), project)` | `project` of the same record, for a related model no resource or option source labels | neither |
| `CountColumn::new(relation!(..))` | the number of a `Deferred` `has_many` field's records | neither |
| `BooleanColumn::new(lens)` | a check or a cross icon for a `bool` field; the export writes `Yes`/`No` (`.labels(..)`) | `.sortable()` |
| `FileColumn::new(lens)` | a `String` field's stored upload path, as a link when it is a rooted path or an `http(s)` URL | neither |
| `EmbeddedColumn::new(lens)` | an embedded value's fields as `Label: value, …`; an enum's variant first, then that variant's fields | neither |

```rust
{{#include ../../../examples/guide/src/tables.rs:table-format}}
```

- **Labels.** A field or relation column is labelled from its field name (`created_at` →
  "Created at"), or `.label(..)`; a computed column uses the label you pass. A relation column's
  `.label(..)` also renames it, so two columns over one relation need two labels.
- **Blank values.** A blank value renders as a muted `—`, in the table and on the detail page; the
  export leaves the cell empty.
- **Detail pages.** The same columns build a resource's [detail page](./detail-pages.md).
- **Relations.** A relation column declares the include its `relation!` names, so the list, the
  export and the detail page load the related records with the page's rows. A relation no column
  includes is not loaded; a `ComputedColumn` whose closure reads one, say to combine two, declares
  each with `.include(..)`.

```rust
{{#include ../../../examples/guide/src/tables.rs:table-relation-column}}
```

An include loads related rows through Toasty alone: the related resource's `query` and policy do
not apply, so a relation column shows a record that resource hides, and a `CountColumn` counts it.
A `CountColumn` counts the loaded list, so it loads every related record of the page's rows.

- **Widths.** The table uses a fixed layout: a column's width is what it declares, not the width of
  its widest cell, so paging and filtering never shift the columns. A field or `RelationColumn`
  takes an equal share of the space left over; a computed or count column defaults to a narrow
  share of the table (10%, scaled down when many columns claim one). Override with
  `.width(ColumnWidth::Percent(30))`, `Rem(8)`, `Narrow` or `Wide`. A cell wider than its column
  is truncated with an ellipsis. On a narrow screen the table keeps a minimum width and scrolls
  horizontally instead of narrowing its columns.

Two columns with the same name, two filters with the same name, a select filter option its field's
type does not parse, a table with no columns, and a zero page size are misdeclarations: mounting
the panel refuses the resource, and rendering the table fails with the same errors.

### Your own columns

A column is anything that implements `Column<M>`, from `tablo::extend`. The built-in columns
implement it and nothing more, so a column of your own reaches as far as theirs:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-custom-column}}
```

Only `name`, `label` and `text` are required. The other methods default to a column that is
narrow, not searchable, not sortable and reads no relation: override `column_width`,
`is_searchable` and `search_expr`, `is_sortable` and `order_by`, or `includes` to change that. Put
the column in the tuple next to the built-in ones, or append it with `Table::column(..)`, which
also takes columns past the tuple's twelve.

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
- `.hide_search()` removes the search box, and `.hide_filter_bar()` the filter controls.

### Live updates

Every table updates in place. The page keeps the table's query in a Topcoat signal: typing in the
search box (once the reader pauses), choosing a filter, sorting and paging write it, and the runtime reruns the page with
the new state, keeping focus and scroll position. The address bar keeps the URL the page opened
with. Without JavaScript the links still navigate to the state they spell, and Enter submits the
search and filters as a GET form.

## Filters

```rust
{{#include ../../../examples/guide/src/tables.rs:table-filters}}
```

| Filter | Field | Values |
| --- | --- | --- |
| `SelectFilter::of(lens)` | an [`Options`](./forms.md#controls) enum, or an `Option` of one | one of the type's options |
| `SelectFilter::new(lens, options)` | `String`, or any form scalar | one of `options`, matched exactly; the options are `Vec<(String, String)>` (an `Options` list), `Vec<String>`, or `[&str; N]` |
| `TernaryFilter::new(lens)` | `bool` | `true`, `false`, or `all` (no filter) |
| `DateFilter::new(lens)` | `jiff::Timestamp` | a date `2024-01-15` matches that UTC day; an RFC 3339 timestamp matches that instant |
| `QueryFilter::new(name, label).option(label, predicate)` | any | named options, each a Toasty predicate you build |

Each active filter is one parameter, `?f.<name>=<value>`, named after the field (a
`QueryFilter` after its name), and active
filters combine with AND.

A filter is anything that implements `Filter<M>`, from `tablo::extend`: a name, a label, the
predicate a value selects, and the control the filter bar renders. The four filters above implement it and nothing more.

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
row count; a header reads an `Options` enum by its label. Grouping runs on the loaded page, so the
counts cover that page, not the whole table. A `?group_by=` value the table does not declare is
ignored.

## Export

`GET /admin/{slug}/export` returns the list as a CSV file named `{slug}.csv`, with the current
search, filters and sort applied, and the relations the columns include loaded. The list header
links it as **Export CSV**, carrying the state the reader sees.

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
- each [custom action](./actions.md) placed on rows that the record allows.

A row that allows none keeps an empty actions cell.

When the policy allows `DeleteAny`, or `RunAny` for a [custom action](./actions.md) the resource
places on the bulk bar, the list adds a checkbox column and a bulk bar. A row that neither delete
nor any bulk action allows gets no checkbox, so select-all only selects rows something can be done to. A bulk delete
accepts at most 400 records and runs in one transaction. It skips the selected records the policy
refuses `Delete` and reports them in its notification (`"Bulk deleted (1 of 2 skipped)"`); a
selection of refused records only deletes nothing and returns to the list with an error
notification.

Both deletes require confirmation. The Delete action opens a confirmation dialog on the list page,
as it does in the header of the record's [detail and edit pages](./actions.md#on-a-records-pages),
and the bulk bar's button, disabled while nothing is selected, opens one stating how many rows are
selected. Confirming posts the write, which returns to the list as it was left (its search, sort
and filters, on the first page) with a notification. The delete handlers refuse a POST that was not
confirmed through the dialog with 400. The dialogs need JavaScript: without it the Delete action
does nothing.

If the table fails to load, the list shows an error state with a retry link in place of the rows;
the rest of the page still renders.
