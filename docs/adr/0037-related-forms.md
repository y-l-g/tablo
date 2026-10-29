# 0037 A has_one relation's form is the related resource's own, written in the owner's transaction

`Relation::has_one::<C>(foreign_key)` declares a record's one child of the resource `C`. Its
section is `C`'s own form, rendered inside the owner's create and edit form after the owner's
fields. Each of its controls posts under `C`'s slug and a dot, as in `profiles.bio`. The foreign
key renders no control: the owner's key fills it.

The child writes through `C` as it would on `C`'s own pages. That means `C`'s policy, tenancy,
field access, control rules, unique probe, `validate_record`, `create_record` or `update_record`,
and `before_commit` and `after_commit`. The owner's hooks receive the owner's change only. A
related form is a second resource's write inside the first one's transaction, not a part of the
owner's record form. The owner's record form keeps describing the owner's columns alone
([0022](0022-record-forms.md)): ADR 0022's exclusion of a relation from a record form stands.

The save writes the owner first, then the child, in one transaction
([0004](0004-action-auth.md)). On a create, the child's foreign key is the owner's new key, which
exists only once the owner is written. The child's controls are checked before the transaction
opens, as the owner's are. The child's parse, `validate_record` and record fn run after the
owner's write, inside the same transaction. A refusal from either side rolls both back and
renders both forms with their errors.

The child's submission is read in its own keys: the panel strips the prefix before `C`'s
pipeline reads the posted pairs. So completing an edit from the stored record, conditions,
field access and errors read the related form's own keys. Only the rendered controls post
under the prefix. Each control takes the name it posts under from where it renders, and the
section carries its own scope for DOM ids and condition signals.

The panel takes no shortcut around `C` for a request it cannot judge:

- The section renders only while the request may write the child. That is `Create` on `C` for a
  record with no child yet, or `View` and `Update` on its child. The foreign key must also be
  neither hidden nor locked by field access.
- Every row holding the owner's key counts, whether or not `C`'s query shows it. When a record
  holds more than one, or one outside `C`'s query, no section renders: a save would otherwise
  create a second child.
- A submission that posts under the prefix while no section renders is refused. So is one that
  posts the foreign key.
- A submission that posts nothing under the prefix leaves the child as it is. An editable cell
  and an import row post the owner's keys only.

Mounting refuses a related form that cannot meet these rules:

- an owner or a related resource with no form;
- a related form that binds no control for the foreign key;
- a soft-deleting related resource, whose trashed child would stay the record's while the form
  created another;
- a related form holding a control that posts keys besides its own or fetches from its
  resource's routes: a file field, a repeater, an embedded value, or a searchable or dependent
  choice;
- an owner form key the prefix would shadow.

A `has_one` relation renders no table: its form edits the record's one child.

## Rejected

- A field of the owner's record form typed as the child's form, as an embedded value binds:
  the child would bypass `C`'s policy, tenancy, field access and hooks, and the owner's form
  would declare the child's controls a second time.
- Toasty's nested create through the owner's create builder: the same bypass, with no matching
  update.
- Writing the child before the owner: on a create, the owner's key does not exist yet.
- A second form posting to `C`'s own routes: the two writes would run in separate transactions.
- Prefixing a copy of `C`'s schema: the values, errors, field access mask and conditions would all
  need the prefix too. Naming each control where it renders keeps them in `C`'s keys.
- Creating a second child when the record's child is outside `C`'s query: the record would hold
  two, and the hidden one would keep its link.
- `has_many` rows in the owner's form: the relation table edits them already, and its rerun drops
  rows a script added (tablo#515).
