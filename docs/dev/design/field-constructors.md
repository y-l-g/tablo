# Field constructors inferring projection and type from the lens

Closes #385.

Line citations refer to the tree this design was written against, `f9750974`.

## Summary

`TextColumn::field` renders a `String` lens with its identity projection,
`TextInput::typed` drops the turbofish (`T` infers from the lens), and
`Select::checkbox` binds a `bool` lens to a checkbox. The closure-based
`TextColumn::for` and the `String`-only `TextInput::for` stay for fields that
need them.

## Motivation

A field is named in four places: the `RecordForm` struct (`pub name: String`,
`examples/showcase/src/app.rs:177`), the table column with an identity closure
(`TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone())` at
`:126`), the view schema (`TextInput::r#for(User::fields().name())` at `:145`),
and the form schema (`:193`). `TextColumn::for` requires the closure even when
it clones the lens value (`crates/tablo-core/src/resource/column.rs:204-207`),
and every handwritten `TextInput::typed::<M, T>` repeats both generic arguments
(`:156`, `:214`, plus 29 further sites) although `T` sits in the `Path<M, T>`
argument position (`schema/fields/text_input.rs:196-200`). A `bool` renders as
a `Select` with `"true"` / `"false"` options (`app.rs:150-155`) because no
checkbox field kind exists in `schema/fields/`; the only checkbox in the repo
is the `tablo-ui` primitive the `FileUpload` clear toggle reuses
(`schema/fields/file_upload.rs:163-169`).

## User-facing API

```rust
TextColumn::field(User::fields().name()).searchable().sortable()
TextColumn::computed("Status", |u: &User| { /* ... */ })

TextInput::typed(User::fields().age()).label("Age").optional()
Select::checkbox(User::fields().active()).label("Active").optional()
```

`TextColumn::field` takes a `String` lens (`FieldLens<M, String>`,
`schema/lenses.rs:24`), renders its `Display` spelling, and keeps the `for`
width default (`Wide`, `resource/column.rs:216`); `computed` keeps `Narrow`
(`:241`). Reach for the closure-based `for` when the cell formats or combines
fields. `Select::checkbox` takes a `bool` lens and posts the same
`"true"` / `"false"` strings the `Select` posts today, parsed through the same
typed rule (`TypedValue: Display + FromStr`, `schema/validation.rs:22`, with
`bool` at `:57-59`).

## Behavior

`field` is an identity projection over the lens value: no query change, no
search/sort change; `searchable` / `sortable` attach exactly as on `for`. The
turbofish removal is inference only: `T` already appears in the lens argument,
and the identically shaped `typed_context` is called bare today, from the
derive (`crates/tablo-macros/src/embedded.rs:348`) and by hand
(`examples/showcase/src/app.rs:417-420`). The checkbox renders the `tablo-ui`
checkbox primitive (`crates/tablo-ui/src/components/primitives/checkbox.rs:33`)
plus a hidden input with the same name posting `"false"` emitted before the
checkbox, which posts `"true"`. Duplicate keys are last-wins: a checked box
posts both keys and reads `true`; an unchecked box posts only `"false"` and
writes `false`. A box with no posted key at all (no companion, no script)
reads as unnamed and keeps the stored value under the completion rule.

## Edge cases

- **Non-`String` lenses through `field`.** A lens whose leaf is not `String`
  fails to compile; the error points at the `FieldLens<M, String>` bound. The
  generic-`Display` overload stays future work.
- **Optional bools.** An emptied checkbox control takes the field's `blank`
  answer under the record-form rule; a missing answer is a build refusal, as
  for any optional control.
- **Read-only rendering.** The detail page renders `checkbox` as its stored
  label, matching `Select` read-only behavior.

## Alternatives

- **A single derive generating record form, table, and schema from one
  struct.** Removes more repetition but couples list display to form shape. Kept
  as future work behind this additive step.
- **`field` generic over `Display`.** Covers integers and timestamps now but
  adds bound design (width defaults, sort/search semantics per type) this step
  does not need. Deferred.

## Out of scope

`RecordForm` derivation, completion, and the `Table` key constructor (#384) are
unchanged. `TextInput::for` keeps its `String`-only signature. No checkbox
filter control rides along.
