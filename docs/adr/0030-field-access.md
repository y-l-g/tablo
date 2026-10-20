# 0030 Field access is declared per field and masks every page a request renders

A resource restricts one field of its records with
`ResourceDef::field_access(lens!(Employee.salary), FieldAccess::new().view(is_hr).edit(is_payroll))`.
Each rule is a predicate over the request, not the record: a column, a filter or an export shows
or drops a field for the whole page, so a per-record rule would leak through the header, the sort
and the CSV. Editing implies viewing.

A request reads the resource through a mask of root fields, built once from its rules. The table
it lists, sorts, searches, filters, groups and exports is the mounted table without the columns,
filters and grouping that read a hidden field, so a sort, a search or a filter naming one reads as
one naming an unknown column, and the relationship choices over the resource search that same
table. The detail page drops the hidden columns and a block left with none. Hiding a `belongs_to`
relation hides its foreign keys, and hiding a key hides the relation that shows its record.

The mask is the resource's, not the model's. Another resource that projects the record, a
`RelationColumn` or a `ComputedColumn` over a relation, shows what its closure reads; the panel
cannot see which fields that is, and refusing every projection over a restricting resource would
refuse the common one over its title. A `has_many` or a `has_one` is refused: its records are the
related resource's, which its policy restricts, and the relation table and its links read none of
the owner's mask. So is the soft-delete column, whose state the trash control shows.

What a column or a filter reads is its own declaration, `Column::reads` and `Filter::reads`, by
the root field each path starts at. A built-in answers its binding, a relation column its
relation and foreign keys; a `ComputedColumn`, a `QueryFilter` or a `TextColumn` with a
`description` answers what `.reading(..)` declares, and a custom one `None` until it says.
Mounting a resource that restricts a field refuses a column or filter answering `None`, since the panel cannot tell what it shows. For a
field with a `view` rule, it refuses a record title reading it, since every page names the record
by its title, and a form watch over it from a control outside it, since a condition shows what it
watches.

The record form is a typed struct (ADR-0022), so a field the user may not change still takes a
value. Its control renders nothing when hidden and disabled when locked, a locked repeater
nothing, since its rows post its key. A submission posting either key, a row's key or a file flag
answers 400 before anything is read: no browser posts it, so a post is a forged one, and dropping
it silently would hide the forgery. An edit completes the field from the stored record, as it does
any unposted key; a create takes the control's default, else the blank answer. A create needing a
value the user may not choose with no default to take is refused, and the list offers none.

## Rejected

- A field ability in `Policy` (`Ability::ViewField(&str)`): the field names a string the compiler
  does not check, and a policy asked per field per row cannot drop a table's column.
- Per-record rules: a list shows a column for every row or for none, and an export's header is one
  line.
- Dropping a posted restricted key: the form accepted a forged write in silence, and a client that
  posts one is not a browser rendering the form.
- Inferring what a computed column reads: a closure over the record names no field the panel can
  see, so it says or the mount refuses it.
