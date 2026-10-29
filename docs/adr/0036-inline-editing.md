# 0036 An editable cell posts its one key to the record's edit route

A table column edits its field in place with `ToggleColumn`, `SelectColumn` or
`TextInputColumn`. Each names its field through the model lens a read-only column takes, and the
cell posts that field's key, alone, to the record's edit route, the one the edit page posts to.
The route completes every other key from the stored record, so a cell writes through the same
CSRF check, policy, field access, control rules, `validate_record`, unique probe, version
condition, `update_record` and hooks as the page. No second write path exists to fall behind it.

A cell is a form post that lands back on the table, as every other table write does
([0026](0026-topcoat-runtime-only.md)). Each editable cell renders its own form, carrying the
CSRF token, the record's version and the table's `?return=`, so a cell's post carries only its own
key and a row's cells never post each other's. A text input submits on Enter. A toggle or a select
submits on change through the runtime's one line of script. A refused post renders the edit page
with the posted value and its errors, as a refused save from that page does, since an error may
name any field the form validates, not only the cell's. Saving there lands back on the table.

The cell spells its value as the model field's form spelling, as a derived record form hydrates
it. A hand-written `RecordForm` hydrating the key otherwise would read the cell's post its own way,
so it must spell the key as the model field does. The landing keeps the table's filters, sort and
page number but not a cursor pager's position or the scroll, the cost of a post that redirects.

A column binds a field of the model, and the record form owns the write
([0022](0022-record-forms.md)). This ADR extends 0022: the mount now checks a table column against
the form. It refuses an editable column when:

- the resource has no form;
- the record form binds no control for the field;
- the control is of another kind: a toggle needs a checkbox or switch, a select a choice over
  static options, and a text input a one-line text field that is no password;
- a select's form choice offers other options or labels than the column's type, so the cell would
  offer a value the form refuses;
- the control is disabled on the edit form, whose save drops what it posts;
- the control shows only while a condition holds. A condition reads the row's other values, which
  one cell does not post.

A cell renders its control only where the edit page would let the user change the field: on a
row whose record allows `Update` and is not trashed, under a table wired with the edit route,
for a field the request's field access neither hides nor locks. Anywhere else, the cell renders
its value read-only: on a detail page, for a locked field, on a row the policy refuses, or for a
trashed record.

## Rejected

- Binding the cell to the form's control (`c.featured`) as well as the model's lens: a second
  declaration of one field, and a `Table<M>` names no form type to check the control against.
- A cell route of its own: it would repeat the edit route's checks, and the next check added to
  one would be missing from the other.
- Applying the response in place, as a JSON patch of the cell: 0026 rejects it, and the table
  already reruns after a redirect.
- One form per row or around the table: every control in it would post together, and a key
  posted twice reads its last value.
- Showing a refused value's error in the cell: a `validate_record` error may name another field,
  which the table does not show.
- Relationship selects: their options come from the related resource's policy-scoped search,
  which a cell would have to page and filter as the form's control does.
