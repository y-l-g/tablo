# 0030 Field access is declared per field and masks every page a panel renders

A resource restricts one field of its records with
`ResourceDef::field_access(lens!(Employee.salary), FieldAccess::new().view(is_hr).edit(is_payroll))`.
Each rule is a predicate over the request, not the record: a column, a filter or an export shows
or drops a field for the whole page, so a per-record rule would leak through the header, the sort
and the CSV. Editing implies viewing.

The rule restricts the field of its model within the panel, not only on the resource that declares
it: every resource the panel mounts over the model masks that field the same way. A field one
resource hides is hidden on every page, relation column, filter and choice of the panel that shows
its records, and a field one resource locks is locked on every form that edits them. A rule repeated
on each resource that shows the model would leave the field visible on any resource that forgot it,
and a second resource over one model is exactly that case.

A request reads the panel through a mask of root fields per model, built once from every mounted
rule over the model. The table it lists, sorts, searches, filters, groups and exports is the mounted
table without the columns, filters and grouping that read a hidden field, so a sort, a search or a
filter naming one reads as one naming an unknown column, and the relationship choices over the
resource search that same table. The detail page drops the hidden columns and a block left with
none. Hiding a `belongs_to` relation hides its foreign keys, and hiding a key hides the relation
that shows its record.

A relation column shows the related record through the resource it names. `RelationColumn::new::<R>`
names the fields of the record its closure reads, and the mask of the record's model hides one, so
the column drops as its own table would. `of` and `list` show the record's title, which the mount
keeps clear of every hidden field, and `CountColumn` shows no field, so it is never refused.

A column reading related records no mounted resource masks is unmasked: `RelationColumn::unscoped`,
a relation column whose source is not one of the panel's resources, a `ComputedColumn`, or a custom
column. The mount refuses an unmasked column reaching records of a model a mounted resource
restricts a field of. A `ComputedColumn` or a custom column reads its row, which may hold any
relation the page loads, so it is refused on a page whose columns load such records, through any
column's includes. Any other unmasked column is refused when its own includes load such records.
The includes are read off the schema through embedded values and enum variants, and a relation the
schema cannot resolve is refused, since the panel cannot tell what it loads.

A relationship choice or filter whose options show records under a custom label, or under the label
of a source that is not a mounted resource, is refused over such a model, since its options show
values the panel cannot mask. A dependent choice narrowed by a field a mounted resource restricts is
refused: its options would show which records hold each value of the field. A query filter whose
reads reach such a record through a relation is refused, since its query can match on a field a
request may not view; a relationship filter over the key filters through the relation instead.

A record title may not read a field a mounted resource restricts, since every page and relationship
choice names the record by its title, and a key may not be restricted at all: the key is every
record's URL id and its default label. The soft-delete column may not be restricted, and neither may
a `has_many` or a `has_one`: its records are the related resource's, which its policy restricts, and
the relation table and its links read none of the owner's mask. The soft-delete column's state is
what the trash control shows.

What a column or a filter reads is its own declaration, `Column::reads` and `Filter::reads`, by the
root field each path starts at. A built-in answers its binding, a relation column its relation and
foreign keys; a `ComputedColumn`, a `QueryFilter` or a `TextColumn` with a `description` answers what
`.reading(..)` declares, and a custom one `None` until it says. Mounting a panel that restricts a
field refuses a column or filter answering `None`, since the panel cannot tell what it shows. A
form watch over a restricted field from a control outside it is refused too, since a condition shows
what it watches.

The record form is a typed struct (ADR-0022), so a field the user may not change still takes a
value. Its control renders nothing when hidden and disabled when locked, a locked repeater nothing,
since its rows post its key. A submission posting either key, a row's key or a file flag answers 400
before anything is read: no browser posts it, so a post is a forged one, and dropping it silently
would hide the forgery. An edit completes the field from the stored record, as it does any unposted
key; a create takes the control's default, else the blank answer. A create needing a value the user
may not choose with no default to take is refused, and the list offers none.

## Rejected

- A field ability in `Policy` (`Ability::ViewField(&str)`): the field names a string the compiler
  does not check, and a policy asked per field per row cannot drop a table's column.
- Per-record rules: a list shows a column for every row or for none, and an export's header is one
  line.
- A mask per resource, which an earlier revision of this decision chose: a field one resource hid
  stayed visible on every other resource over its model, so a relation column naming one of them, a
  filter or a write through its form showed or changed the value the first resource restricted.
- Dropping a posted restricted key: the form accepted a forged write in silence, and a client that
  posts one is not a browser rendering the form.
- Inferring what a computed column reads: a closure over the record names no field the panel can
  see, so it says or the mount refuses it.
