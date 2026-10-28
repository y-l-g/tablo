# Tables

A resource's `Table`, set with `ResourceDef::table`, declares its list page: the columns, and
the search, sort, filter, grouping and pagination the list offers. It depends on no request: the
panel builds it once when it mounts and serves that table to every request. The same declaration
drives the CSV export.

The table defaults to the model's, `User::table()`, which
[`#[derive(tablo::ModelColumns)]`](https://y-l.fr/tablo/nightly/api/tablo_core/derive.ModelColumns.html) writes: a sortable column per
field, whether a form or Toasty writes it, so a `#[default]` `created_at` is listed too, and
`#[columns(skip)]` leaves out a field such as a password hash. Extend the model's table, whose
`filters(..)` add to the ones a field's `#[columns(filter)]` declares, or declare the columns
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

Each row is keyed by its record's primary key, so a model with a composite key lists without row
actions or bulk selection.

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

The built-in columns cover the common cells: `TextColumn` for any value, `DateColumn` for a
calendar day, `BooleanColumn`, `FileColumn` and `ImageColumn` for uploads, `RichTextColumn`,
`ColorColumn`, `EmbeddedColumn` for an embedded value, `ComputedColumn` for what your closure
returns, and `RelationColumn` and `CountColumn` for related records. Each type's page in the
[API docs](https://y-l.fr/tablo/nightly/api/tablo/index.html#structs) says what its cell shows, what the export writes, and whether
it searches and sorts.

```rust
{{#include ../../../examples/guide/src/tables.rs:table-date}}
```

- **Labels.** A field or relation column is labelled from its field name (`created_at` →
  "Created at"), or `.label(..)`.
- **Detail pages.** The same columns build a resource's [detail page](./detail-pages.md).
- **Relations.** A relation column declares the include its `relation!` names, so the list, the
  export and the detail page load the related records with the page's rows. A relation no column
  includes is not loaded; a `ComputedColumn` whose closure reads one, say to combine two, declares
  each with `.include_unscoped(..)`, and reads it under no resource's query or policy.

```rust
{{#include ../../../examples/guide/src/tables.rs:table-relation-column}}
```

A relation column over `R` shows only the related records `R` shows the user, as a relationship
choice over `R` offers them, so a list never leaks another tenant's author or a comment the user
cannot view ([`RelationColumn`](https://y-l.fr/tablo/nightly/api/tablo/struct.RelationColumn.html)). A `CountColumn` counts the
loaded list, so it loads every related record of the page's rows.

- **Widths.** The table lays out automatically, a column as wide as its content;
  `.width(..)` takes a [`ColumnWidth`](https://y-l.fr/tablo/nightly/api/tablo/enum.ColumnWidth.html) to wrap a long value or fix a
  width.
- **Rows.** A row opens its record's view page, or its edit page when it has no view page,
  wherever it is clicked; the checkbox and the action buttons keep their own click.
- **Row actions.** A row's actions sit side by side at its end;
  `.action_layout(ActionLayout::Menu)` puts them behind one button instead.

A misdeclared table, such as two columns with the same name, fails the mount, and rendering it on
your own page fails with the same errors.

### Presenting values

A `TextColumn` presents its value beyond plain text:

`.badge()` renders the value as a badge, `.color(..)` colors it, `.description(..)` adds a muted
second line, `.numeric(decimals)` and `.money(currency)` format a number, and `.align(..)` moves
it to the end; [`TextColumn`](https://y-l.fr/tablo/nightly/api/tablo/struct.TextColumn.html) has each method's rules.

An `Options` enum gives each variant its badge color with `#[option(color = "…")]`, one of
`gray`, `primary`, `success`, `warning`, `danger` and `info`. Each paints with a
[theme token](./panel-and-routing.md#theme-tokens), so a theme recolors every badge at once:

```rust
{{#include ../../../examples/guide/src/models.rs:post-status-options}}
```

```rust
{{#include ../../../examples/guide/src/tables.rs:table-presenting}}
```

A cell can also link to a URL, copy its value, show a tooltip, cut a long value, show an icon or a
placeholder for an empty value, and a table can tint a row by its record:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-cell-options}}
```

A link takes a relative URL or an `http`, `https`, `mailto` or `tel` one, so a value a user wrote
cannot turn into a script. A closure over the row may read any field: on a resource that
[restricts a field](./resources.md#field-access), it declares what it reads with `.reading(..)`, or
`.row_color_reading(..)` for the row color, as a `.description(..)` does.

The export writes the formatted value: `$1,234.50`, not `123450`. Money uses `,` and `.` as
separators; a locale's own spelling is a `.format(..)`. Over a [decimal column](./forms.md#typed-values),
`.numeric(..)` and `.money(..)` spell the stored digits without passing through a float.

### Hiding columns

`.toggleable()` lets each reader hide a column from the list, and `.hidden_by_default()` starts it
hidden until they show it. Every built-in column takes both:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-toggleable}}
```

A list with such a column renders a Columns menu in its toolbar. The panel keeps each signed-in
user's choice per resource, in the shipped `ColumnPreference` model, which `tablo::models!`
registers. Hiding a column only declutters the list: search, sort and the export still read it,
and the detail page shows it.

### Presenting a derived column

A model whose default table fits but for a label, a badge or a hidden column keeps the derive and
says so on the field, rather than declaring every column. Each `#[columns(..)]` key calls the
builder of the same name on the column the field's type picks, in the table and on the detail
page, and `filter` adds the filter the type picks:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-derived-keys}}
```

A key the column does not take, such as `badge` on a `bool`, fails to compile at the key. The
[derive's page](https://y-l.fr/tablo/nightly/api/tablo_core/derive.ModelColumns.html#per-field-attributes)
lists the keys and the filter each type picks.

### Your own columns

A column is anything that implements `Column<M>`, from `tablo::extend`. The built-in columns
implement it and nothing more, so a column of your own reaches as far as theirs:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-custom-column}}
```

Only `name`, `label` and `text` are required;
[`Column`](https://y-l.fr/tablo/nightly/api/tablo/extend/trait.Column.html) lists the methods that make it searchable, sortable,
toggleable or loading a relation. A sortable column of your own is the table's default
sort through `default_sort_by(Sort::desc("revenue"))`, which names it by its `name`. Put
the column in the tuple next to the built-in ones, or append it with `Table::column(..)`, which
also takes columns past the tuple's twelve.

## Search, sort and pagination

The URL holds the list's whole state, so every view of a list is a link you can share:

`?q=` searches, `?sort=` and `?dir=` sort, `?page=` and `?per_page=` page, `?f.<name>=`
filters and `?group_by=` groups. [`Table`](https://y-l.fr/tablo/nightly/api/tablo/struct.Table.html) has the parameters' rules: how
search matches, the default sort, and what an unusable filter does.

A table shows 25 rows per page unless `.paginate(n)` sets another size, and offers the reader a
choice of sizes that `.per_page_options([..])` changes. Each numbered page counts the result set,
one more query, so a table too large to count, or one rows are written to while it is read, picks
another [`Pagination`](https://y-l.fr/tablo/nightly/api/tablo/enum.Pagination.html): `Simple` drops the count, and `Cursor` pages by
row, so no row is skipped or repeated when rows change between two pages.

`.hide_search()` removes the search box, and `.hide_filter_menu()` the filter menu.

### Live updates

Every table updates in place: typing in the search box, choosing a filter, sorting and paging
rerun the page with the new state, keeping focus and scroll position. Without JavaScript the
links still navigate to the state they spell.

## Filters

```rust
{{#include ../../../examples/guide/src/tables.rs:table-filters}}
```

`SelectFilter` matches one or more values of a field, `TernaryFilter` a `bool`, `DateFilter` a
day or a range of days, and `QueryFilter` named options, each a Toasty predicate you build. An
active filter's parameter is `?f.<name>=<value>`, named after the field, and active filters
combine with AND. Each filter's page has its values and its options, such as `.multiple()` for a
checklist or `DateFilter::range` for two bounds.

Filter a foreign key through
[`SelectFilter::relationship`](https://y-l.fr/tablo/nightly/api/tablo/struct.SelectFilter.html#method.relationship): it offers and
applies only the related rows the user may view, as a relationship choice does, where a plain
`SelectFilter` over the key would reveal which keys exist.

A filter is anything that implements `Filter<M>`, from `tablo::extend`: a name, a label, the
predicate a value selects, and the control the filter menu renders. The four filters above implement it and nothing more.

```rust
{{#include ../../../examples/guide/src/tables.rs:table-promoted-filter}}
```

`FilterInput` carries the parameter the control submits and the current `FilterValue`, and
`input.select(cx, options)` renders the built-in select, `input.checklist(cx, options)` the
built-in checklist. A control offering what a query loads overrides `load_control`, the async
method the filter menu calls, which defaults to `control`. `value.single()` is the one value a
single-choice control submits, `value.values()` every repeated value, and `value.part("from")` a
part a control submits as `?f.<name>.from=`, under `input.part_param("from")`:

```rust
{{#include ../../../examples/guide/src/tables.rs:table-adults-filter}}
```

A filter of your own goes in the `filters((..))` tuple; alone, it is a one-element tuple,
`.filters((Adults,))`. Its name holds no `.`, which the parameter reserves for a part.

A filter that cannot apply is never dropped silently: the list shows a warning banner naming it,
and the export refuses the request rather than export more rows than asked.

## Grouping

```rust
{{#include ../../../examples/guide/src/tables.rs:table-group-by}}
```

`?group_by=status`, named after the field, groups the current page's rows under headers with a
row count. Grouping runs on the loaded page, so the counts cover that page, not the whole table.

## Export

The list header links **Export CSV**, which downloads the list as the reader sees it: the current
search, filters and sort, only the rows the policy lets them view, and at most 10,000 rows by default
([`Limits::export_rows`](https://y-l.fr/tablo/nightly/api/tablo/struct.Limits.html)), never a
truncated file. [`Table`](https://y-l.fr/tablo/nightly/api/tablo/struct.Table.html#export) has the export's rules.

## Import

A resource that creates through a form can import a CSV file too:

```rust,ignore
ResourceDef::new().importable()
```

The list header then links **Import CSV** for a request the policy allows to `Create`. The file's
header names the form's keys, and each row is a create submission, checked by the same field
access, rules, `validate_record` and record fn as the create page. Every row writes in one
transaction, so one refused row writes nothing, and the page lists every refusal by row and
column:

```csv
name,email,role
Barbara Liskov,barbara@example.com,admin
```

[`ResourceDef::importable`](https://y-l.fr/tablo/nightly/api/tablo/struct.ResourceDef.html#method.importable) has the file's rules,
its limits and what the hooks receive. The import reads what the form posts, not what the export
writes, so an exported file does not import back.

## Row actions and deletes

Each row shows View, Edit, Delete and the [custom actions](./actions.md) placed on rows, each as
the policy allows it on that record. When the policy allows `DeleteAny`, or a custom action on the
bulk bar, the list adds a checkbox column and a bulk bar, which shows in the toolbar once a row is
selected. A row nothing can be done to gets no checkbox, so select-all only selects rows something
can be done to.

A bulk delete skips the records the policy refuses and reports them in its notification. Both
deletes ask first through a confirmation dialog, as in the header of the record's
[detail and edit pages](./actions.md#where-it-shows), and land back on the list as it was left.
[`Table`](https://y-l.fr/tablo/nightly/api/tablo/struct.Table.html) has the limits.

If the table fails to load, the list shows an error state with a retry link in place of the rows.
