# Tables

A resource's `Table`, set with `ResourceDef::table`, declares its list page: the columns, and
the search, sort, filter, grouping and pagination the list offers. It depends on no request: the
panel builds it once when it mounts and serves that table to every request. The same declaration
drives the CSV export.

The table defaults to the model's, `User::table()`, which `#[derive(tablo::ModelColumns)]` on the
model writes: a sortable column per field, whether a form or Toasty writes it, so a
`#[default]` `created_at` is listed too. A `String` or `Option<String>` column is searchable, an
`Options` enum shows its label, and a `bool` shows as yes or no. A key Toasty generates, a
relation and its foreign key, a `TenantId`, an embedded value and a list get no column, and
`#[columns(skip)]` leaves out any other field. Extend the model's table, or declare the columns
yourself:

```rust
ResourceDef::new().table(User::table().filters(TernaryFilter::new(lens!(User.active))))
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
| `TextColumn::new(lens)` | the field's value as text (an `Options` enum's label), or `.format(\|value\| ..)` of it; see [Presenting values](#presenting-values) | `.searchable()` on a `String` field, `.sortable()` |
| `DateColumn::new(lens)` | a `jiff::civil::Date`, `civil::DateTime` or `Timestamp` field's calendar day, `Oct 14, 2026` (a timestamp's day in the request's [time zone](./forms.md#time-zones)); a dash when the field holds none. The export writes the stored value, `2026-10-14T09:30:00Z`. | `.sortable()` |
| `ComputedColumn::new(label, project)` | `project(row)`; a panel refuses it on a page whose columns load the records of a model a mounted resource restricts a field of | neither: the methods do not exist |
| `RelationColumn::of::<R>(relation!(..))` | a `Deferred` `belongs_to` or `has_one` field's record, titled as `R` titles it: a resource's `record_title`; a dash for a nullable field holding none | neither |
| `RelationColumn::new::<R>(relation!(..), reads, project)` | `project` of the same record, which reads the fields `reads` names: a `lens!` or a `Reads` | neither |
| `RelationColumn::list::<R>(relation!(..))` | a `Deferred` `has_many` field's records, a many-to-many `via` field's included, each titled as `R` titles it, joined by commas | neither |
| `CountColumn::of::<R>(relation!(..))` | the number of a `Deferred` `has_many` field's records | neither |
| `RelationColumn::unscoped(relation!(..), project)`, `CountColumn::unscoped(relation!(..))` | as above, for a related model no resource administers; a panel refuses `RelationColumn::unscoped` over a model a mounted resource restricts a field of | neither |
| `BooleanColumn::new(lens)` | a circled check or circled cross for a `bool` field; the export writes `Yes`/`No` (`.labels(..)`) | `.sortable()` |
| `FileColumn::new(lens)` | a `String` field's stored upload path, as a link when it is a rooted path or an `http(s)` URL | neither |
| `ImageColumn::new(lens)` | a rooted path as a 40px thumbnail (`.size(px)`, `.circular()`); an `http(s)` URL too once `.remote()` allows it, for a field only trusted users write, since every viewer's browser loads it from that host | neither |
| `RichTextColumn::new(lens)` | a [rich text](./forms.md#rich-text) field's text, cut to 120 characters (`.excerpt(chars)`); a detail page shows its cleaned HTML, and the export its text | neither |
| `ColorColumn::new(lens)` | a `String` field's CSS color as a swatch beside its value; a value that is no hex color or color function shows as text | neither |
| `EmbeddedColumn::new(lens)` | an embedded value's fields as `Label: value, …`; an enum's variant first, then that variant's fields | neither |

```rust
{{#include ../../../examples/guide/src/tables.rs:table-date}}
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
  each with `.include_unscoped(..)`, and reads it under no resource's query or policy.

```rust
{{#include ../../../examples/guide/src/tables.rs:table-relation-column}}
```

A relation column over `R` shows only the related records `R` shows the user, as a relationship
choice over `R` offers them: the ones `R`'s tenant-scoped `query` returns and its policy lets the
user `View`. Once the page's rows load, the columns ask `R` which of the loaded records it shows,
one query per 500 records not yet asked about in the request, and render only those: a
soft-deleted author renders empty, and a `CountColumn` leaves out the comments the user cannot
view. The rows keep what the include loaded, so a policy reading the relation sees it whole.
`unscoped` columns show whatever the include loads. A `CountColumn` counts the loaded list, so it
loads every related record of the page's rows.

- **Widths.** The table lays out automatically: a column is as wide as its content, on one line.
  A text, field or `RelationColumn` grows: it caps its value at 20rem, truncating past that, and
  the first growing column takes the width left over. The selection checkbox and the row actions
  shrink to their controls. Override with `.width(ColumnWidth::Auto)` for content width,
  `Wrap` to wrap a long value over several lines, or `Rem(8)` for an exact width. A table wider
  than its card scrolls sideways.
- **Rows.** A row opens its record's view page, or its edit page when it has no view page,
  wherever it is clicked; the checkbox and the action buttons keep their own click.
- **Row actions.** A row's actions sit side by side at its end: View, Edit and Delete as icons,
  a custom action by its label. `.action_layout(ActionLayout::Menu)` puts them behind one button
  instead, Delete last; the menu opens without JavaScript and confirms and asks for input as the
  buttons do.

Two columns with the same name, two filters with the same name, a select filter option its field's
type does not parse, a table with no columns, and a zero page size are misdeclarations: mounting
the panel refuses the resource, and rendering the table fails with the same errors.

### Presenting values

A `TextColumn` presents its value beyond plain text:

| Method | Cell |
| --- | --- |
| `.badge()` | an outline badge, led by a dot in the value's color |
| `.color(\|value\| Some(Color::Success))` | the value's color: the badge's dot, or the text when there is no badge |
| `.description(\|row\| ..)` | a muted second line under the value; search, sort and export read the value alone |
| `.numeric(decimals)` | `12,345.60`, right-aligned with equal-width digits, on any integer or float field |
| `.money(Currency::USD)` | `$1,234.50`, the same way; `Currency::EUR`, `GBP`, `JPY`, or `Currency::new("CHF").symbol_after()`, and `.minor_units()` for a field holding cents |
| `.align(Alignment::End)` | the header and cells at the end; a `ComputedColumn` takes it too |

An `Options` enum gives each variant its badge color with `#[option(color = "…")]`, one of
`gray`, `primary`, `success`, `warning`, `danger` and `info`. Each paints with a
[theme token](./panel-and-routing.md#theme-tokens), so a theme recolors every badge at once:

```rust
{{#include ../../../examples/guide/src/models.rs:post-status-options}}
```

```rust
{{#include ../../../examples/guide/src/tables.rs:table-presenting}}
```

The export writes the formatted value: `$1,234.50`, not `123450`. Money uses `,` and `.` as
separators; a locale's own spelling is a `.format(..)`. A date column is the exception: its export
writes the stored value, `2026-10-14T09:30:00Z`, while its cell shows the day.

### Hiding columns

`.toggleable()` lets each reader hide a column from the list, and `.hidden_by_default()` starts it
hidden until they show it. Every built-in column takes both:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-toggleable}}
```

A list with such a column renders a Columns menu at the right end of its toolbar, checking each toggleable
column the reader sees; Apply keeps the checked ones and Reset restores the defaults. The panel
keeps the choice per signed-in user and per resource, in the shipped `ColumnPreference` model,
which `tablo::models!` registers beside `AuthSession`. The relation tables over a resource show the
columns its list shows. A column added later, or one whose default changes, takes its default
until the reader picks otherwise.

Search, sort and the export still read every column, hidden or not, and a detail page shows them
all. A panel without auth has no user to keep a choice for: it renders no menu and shows the
defaults, as does a table an app renders on its own page with `wired_table`.

### Your own columns

A column is anything that implements `Column<M>`, from `tablo::extend`. The built-in columns
implement it and nothing more, so a column of your own reaches as far as theirs:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-custom-column}}
```

Only `name`, `label` and `text` are required. The other methods default to a column that is
narrow, start-aligned, not searchable, not sortable, always shown and reads no relation: override
`column_width`, `alignment`, `is_searchable` and `search_expr`, `is_sortable` and `order_by`,
`toggle`, or `includes` to change that. A sortable column of your own is the table's default
sort through `default_sort_by(Sort::desc("revenue"))`, which names it by its `name`. Put
the column in the tuple next to the built-in ones, or append it with `Table::column(..)`, which
also takes columns past the tuple's twelve.

## Search, sort and pagination

The URL holds the list's whole state, so every view of a list is a link you can share:

| Parameter | Effect |
| --- | --- |
| `?q=ada` | search: each searchable column contains `ada`; any column may match |
| `?sort=name&dir=desc` | sort by a sortable column; `dir` is `asc` (default) or `desc` |
| `?page=3` | the page, from 1; page 1 spells no parameter |
| `?per_page=50` | the page size, when the table offers it |
| `?after=…`, `?before=…` | the next or previous page of a cursor table, as an opaque cursor |
| `?f.status=published` | a filter: see [Filters](#filters) |
| `?group_by=status` | grouping: see [Grouping](#grouping) |

- Without a `?sort=` naming a sortable column, the rows sort by the table's
  `.default_sort(lens)` (smallest first) or `.default_sort_desc(lens!(Post.created_at))`, else by
  the first sortable column, ascending, and the header marks that column. A default sort on a field
  with no sortable column fails the mount.
- Search escapes `%` and `_`, so they match literally. Terms are trimmed and capped at 128
  characters. Matching is case-insensitive: PostgreSQL runs it as `ILIKE`, and SQLite (for ASCII)
  and MySQL (under its default collations) as `LIKE`.
- A table shows 25 rows per page unless `.paginate(n)` sets another size. Under the rows it says
  "Showing 26–50 of 312", offers 10, 25, 50 or 100 rows per page (`.per_page_options([..])`
  changes the choice, `[]` removes it), and links the first and last pages, the current one and
  its neighbors. A page number past the last page reads as the last, so a stale link lands on
  rows. The primary key breaks ties, so a sort over duplicate values still pages
  deterministically.
- `.pagination(..)` picks how a table pages. Each numbered page counts the result set, one more
  query; a table too large to count, or one rows are written to while it is read, pages another
  way:

| `Pagination::` | Pager | Cost |
| --- | --- | --- |
| `Pages` (default) | "Showing 26–50 of 312", page links, Previous and Next | a count and the page, by offset |
| `Simple` | "Page 2", Previous and Next | the page and one row more, by offset |
| `Cursor` | Previous and Next | the page and one probe row, by cursor: no row skipped or repeated when rows are added or removed between two pages, and no offset to scan past |

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
| `SelectFilter::of(lens)` | an [`Options`](./forms.md#controls) enum, or an `Option` of one | one or more of the type's options |
| `SelectFilter::new(lens, options)` | `String`, or any form scalar | one or more of `options`, matched exactly; the options are `Vec<(String, String)>` (an `Options` list), `Vec<String>`, or `[&str; N]` |
| `SelectFilter::relationship::<R>(lens)` | a foreign key to the rows of `R`, a [relationship source](./forms.md#relationships) | one or more keys |
| `TernaryFilter::new(lens)` | `bool` | `true`, `false`, or `all` (no filter) |
| `DateFilter::new(lens)` | `jiff::Timestamp`, `jiff::civil::Date` or `jiff::civil::DateTime`, or an `Option` of one | a date `2024-01-15` matches that day (in the request's [time zone](./forms.md#time-zones) for a timestamp); an RFC 3339 timestamp, or a civil date-time (read in that zone for a timestamp), matches that value; on a `DateFilter::range` filter the parts `from` and `to` bound a range |
| `QueryFilter::new(name, label).option(label, predicate)` | any | one or more named options, each a Toasty predicate you build |

An active filter's parameter is `?f.<name>=<value>`, named after the field (a `QueryFilter`
after its name). A filter taking several values repeats it, `?f.status=draft&f.status=review`,
and a select or query filter matches a record any of them selects; the ternary and date filters
take one value. A `DateFilter::range` filter travels as parts,
`?f.created_at.from=2024-01-01&f.created_at.to=2024-01-31`, each day included and either bound
alone leaving the range open; a single date filter refuses a part, whose range its one input does
not render. Active filters combine with AND.

Each built-in filter but `QueryFilter` takes `.label(..)` over the field's label; a
`QueryFilter` takes its label in `QueryFilter::new(name, label)`. `.multiple()` turns a select or
query filter's control into a checklist, a popover of checkboxes that stays open while the table
reruns; `TernaryFilter::labels` names its two options; `DateFilter::range` renders two date
inputs, and a bound naming an exact time, such as `2024-01-15T09:30:00Z`, keeps a text input, which
a date input would drop. A relationship filter loads its options as a [relationship
choice](./forms.md#relationships) does: the source's tenant-scoped rows the user may view, each
labelled by the source's label, at most `MAX_RELATIONSHIP_OPTIONS` of them. Past that cap, or
when the source refuses the load, it lists only the chosen keys. It applies only the keys of rows
the user may view: within the cap, the keys its options list; past it, the keys one query through
the source's tenant-scoped query returns and its `View` policy allows. Another key is ignored as
one no row holds: the list warns, and the export answers 400. `SelectFilter::new` or `of` over a
foreign key checks no source, so it does not hide that a key links to a row the user cannot view:
filter a foreign key through `relationship`. A query filter's
`option_keyed(key, label, predicate)` keeps its URL value when the label changes.

A filter is anything that implements `Filter<M>`, from `tablo::extend`: a name, a label, the
predicate a value selects, and the control the filter bar renders. The four filters above implement it and nothing more.

```rust
{{#include ../../../examples/guide/src/tables.rs:table-promoted-filter}}
```

`FilterInput` carries the parameter the control submits and the current `FilterValue`, and
`input.select(cx, options)` renders the built-in select, `input.checklist(cx, options)` the
built-in checklist. A control offering what a query loads overrides `load_control`, the async
method the filter bar calls, which defaults to `control`. `value.single()` is the one value a
single-choice control submits, `value.values()` every repeated value, and `value.part("from")` a
part a control submits as `?f.<name>.from=`, under `input.part_param("from")`:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-adults-filter}}
```

A filter of your own goes in the `filters((..))` tuple; alone, it is a one-element tuple,
`.filters((Adults,))`. At most 32 filters apply, each with at most 64 values and parts, each
name, part and value at most 256 bytes. A filter's name holds no `.`, which the parameter
reserves for a part.

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

`GET /admin/{slug}/-/export` returns the list as a CSV file named `{slug}.csv`, with the current
search, filters and sort applied, and the relations the columns include loaded. The list header
links it as **Export CSV**, carrying the state the reader sees.

- Rows the policy may not `View` are left out.
- The export delivers at most 10,000 rows. When the search and filters match more, it answers
  413 rather than a truncated file.
- Cells that a spreadsheet would read as a formula are escaped. Add `?bom=1` to prefix the file
  with a byte-order mark for Excel.

## Import

A resource that creates through a form can import a CSV file too:

```rust,ignore
ResourceDef::new().importable()
```

The list header then links **Import CSV** to `/admin/{slug}/-/import`, for a request the policy
allows to `Create`. The page takes one file, and **Download template** at
`/admin/{slug}/-/import/template` returns the header row it reads. Mounting the panel refuses
`importable` on a resource that serves no create page (`ImportWithoutCreate`).

```csv
name,email,role
Barbara Liskov,barbara@example.com,admin
```

- The first row names the columns, each by its form key or its field's label in any case. The
  columns are the form's controls the request may change, but file fields, repeaters and controls
  disabled on the create page. A header naming any other column, or a label two fields carry,
  refuses the file.
- Each cell posts what the create form's control would: an option's value, `true` or `false` for
  a checkbox, a date and time in the request's [time zone](./forms.md#time-zones). A blank cell
  posts a blank value. A column the header leaves out takes the field's `default`.
- A multiple choice, such as a many-to-many field, takes one column per value: the header repeats
  its key. No other column may repeat.
- The file is UTF-8 text with commas or semicolons between cells. A row whose every cell is blank is
  skipped.

Each row is a create submission: the field access, the controls' rules, the record form's parse,
`validate_record`, the unique check and the relationship check run as on the create page,
`create_record` stamps the tenant, and `before_commit` receives each row's `Change::Create` inside
the transaction. A row `create_record` refuses with `WriteError::Refused` rolls back to a savepoint
taken before it, so the rows after it are checked without its writes. Every row writes in one transaction. When any row is refused, nothing is written,
and the page lists each refusal of every row by row number, as a spreadsheet numbers it, and
column. Correct the
file and import it again. A `before_commit` error rolls back the whole file. When every row
passes, the import lands on the list and `after_commit` receives each row's `Change::Create`, in
file order.

A file holds at most `MAX_IMPORT_ROWS` (1,000) rows, 500 cells a row and 10 MiB. The
transaction lasts the whole file: on SQLite it holds the database's write lock that long. The import reads what the form
posts, not what the export writes: an exported `$1,234.50`, `Yes` or option label does not import
back.

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
selected. Confirming posts the write, which returns to the list as it was left (its search, sort,
filters and page) with a notification. The delete handlers refuse a POST that was not
confirmed through the dialog with 400. The dialogs need JavaScript: without it the Delete action
does nothing.

If the table fails to load, the list shows an error state with a retry link in place of the rows;
the rest of the page still renders.
