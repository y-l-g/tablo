# Field constructors inferring projection and type from the lens

Closes #385.

Line citations refer to the tree this design was written against, `f9750974`.
Citations under `crates/toasty/` and `crates/toasty-macros/` name the pinned
Toasty checkout.

## Summary

`TextColumn::field` renders a `String` lens without a projection closure,
`TextInput::typed` drops the turbofish (`T` infers from the lens), and
`Select::checkbox` binds a `bool` lens to a checkbox. The closure-based
`TextColumn::for` and the `String`-only `TextInput::for` stay for the cases that
need them.

## Blocker

`TextColumn::field` cannot be implemented at the pinned Toasty revision. A
`TextColumn` renders its cell through `project: Arc<dyn Fn(&M) -> String>`
(`crates/tablo-core/src/resource/column.rs:162`, called at `:329-331`), and
nothing can build that closure from a `Path<M, String>`: no API reads a field
value off a model instance.

The gap, at the pinned revision:

- `Model` exposes schema and path metadata only — `id()`, `schema()`,
  `path_root()`, `path_field()`, `field_name_to_id()`
  (`crates/toasty/src/schema/model.rs:60-115`). Its generated per-field
  accessors return paths
  (`crates/toasty-macros/src/model/expand/fields.rs:537`), and the derive
  generates no per-field value getter
  (`crates/toasty-macros/src/model/expand/model.rs:56`).
- `Path<T, U>` holds an untyped path plus `PhantomData`
  (`crates/toasty/src/stmt/path.rs:37-40`); its methods are query builders
  (`eq`, `lt`, `asc`, `like`, `is_some`, …). It implements `Debug` (`:800`),
  not `Display`, and carries no value reader.
- `TextColumn` documents the same absence and the hand-written closure as the
  only way to read a field (`column.rs:144-148`); ADR-0001 records it as an open
  upstream gap (`docs/adr/0001-typed-field-lenses.md:18-25`).

Tracked in GH #119, which records no upstream issue.

**Upstream API this design needs.** A way to read a field value from an instance
through a lens, so that a constructor taking `Path<M, T>` can produce the
projection the render side consumes — for example a trait implemented by the
derive that maps a path to its value (sketch, not compiling code):

```rust
pub trait FieldValue<M, T> {
    /// Read this field off `model`.
    fn get(&self, model: &M) -> T;
}

impl<M> TextColumn<M> {
    pub fn field(path: FieldLens<M, String>) -> Self
    where
        FieldLens<M, String>: FieldValue<M, String>,
    {
        let project = move |m: &M| path.get(m);
        // name, label, width as `for`
    }
}
```

The exact signature is Toasty's to choose. The requirement is one call that
turns a lens into an instance reader; the derive already knows the field index
and the accessor, so it can emit it.

Until that API exists, `field` is not implementable and the `String` half of
#385 stays blocked. `TextInput::typed` and `Select::checkbox` are independent of
this blocker and merge on their own.

## Motivation

A model field is named in four places: the `RecordForm` struct
(`pub name: String`, `examples/showcase/src/app.rs:177`), the table column with
an identity closure
(`TextColumn::r#for(User::fields().name(), |u: &User| u.name.clone())` at
`:126`), the view schema (`TextInput::r#for(User::fields().name())` at `:145`),
and the form schema (`:193`). `TextColumn::for` requires the closure even when
it clones the lens value (`crates/tablo-core/src/resource/column.rs:204-207`),
and every handwritten `TextInput::typed::<M, T>` repeats both generic arguments
(`:156`, `:214`, plus 20 further occurrences, 18 of them in tests) although `T`
sits in the `Path<M, T>` argument position
(`schema/fields/text_input.rs:196-200`). A `bool` renders as a checkbox through
`Field::toggle` (`schema/fields/custom.rs`).

## User-facing API

```rust
TextColumn::field(User::fields().name()).searchable().sortable()
TextColumn::computed("Status", |u: &User| { /* ... */ })

TextInput::typed(User::fields().age()).label("Age").optional()
Select::checkbox(User::fields().active()).label("Active").optional()
```

`TextColumn::field` takes a `String` lens (`FieldLens<M, String>`,
`schema/lenses.rs:24`) and reads the field value through the instance reader the
Blocker describes; it keeps the `for` width default (`Wide`,
`resource/column.rs:216`), and `computed` keeps `Narrow` (`:241`). Reach for the
closure-based `for` while the Blocker stands, and afterwards for cells that
format or combine model fields.

`TextInput::typed` keeps its signature
(`schema/fields/text_input.rs:196-200`); only the turbofish goes, because `M`
and `T` are both fixed by the lens argument. `TextInput::for` keeps its
`String`-only signature (`:108-111`): `String` implements no `TypedValue`
(`schema/validation.rs:46-81`), so the two constructors bind disjoint lens
types.

`Select::checkbox` takes a `bool` lens and posts the `"true"` / `"false"`
strings `Select::r#for` posts for a bool column today (`app.rs:150-155`). It
gives `Select` the typed rule the field kind lacks: `Select` carries no parser
field (`schema/fields/select.rs:29-46`) and validates presence only
(`:349-351`), so the checkbox installs `Rules::typed::<bool>()`
(`schema/validation.rs:190-197`).

## Behavior

`field` adds no query change and no search/sort change: `searchable` and
`sortable` attach exactly as on `for`, and the predicates come from the lens
alone (`resource/column.rs:342-364`).

The turbofish removal is inference only. `T` already appears in the lens
argument, and the identically shaped `typed_context`
(`schema/fields/text_input.rs:234-238`) is called bare today, from the derive
(`crates/tablo-macros/src/embedded.rs:348`) and by hand
(`examples/showcase/src/app.rs:417-420`).

`Select::checkbox` renders the `tablo-ui` checkbox primitive
(`crates/tablo-ui/src/components/primitives/checkbox.rs:33`) with
`value="true"`, and renders a hidden input of the same name before it to post
`"false"`.
Decoding collects posted pairs into a map
(`panel/forms/decode.rs:367-369`), so a duplicate key resolves to the last pair
in the body: a checked box posts `"false"` then `"true"` and reads `true`; an
unchecked box posts only `"false"` and writes `false`. Multipart bodies resolve
duplicates the same way (`:77`, `:166`), so a form carrying a `FileUpload`
elsewhere keeps the rule.

Because the companion always posts, the checkbox always names its key, so the
completion rule never fills it: a submission without a key at all (no companion,
no script) leaves the key unnamed, and on edit the stored value is completed in
(`panel/forms/submit.rs:102`, `:120-137`). On create nothing is stored, so that
key takes the field's blank answer (`panel/forms/submit.rs:66-68`).

## Edge cases

- **Non-`String` lenses through `field`.** A lens whose leaf is not `String`
  fails to compile; the error points at the `FieldLens<M, String>` bound. The
  generic-`Display` overload stays future work.
- **Traversal lenses.** A lens that crosses an embedded step type-checks but is
  refused at build, as for `for`: the single-field lens rule
  (`schema/lenses.rs:655-662`) surfaces through `Panel::build` as a declaration
  error (fixture `panel/build.rs:1552-1560`, asserted at `:1590-1593`).
- **Nullable strings.** An `Option<String>` leaf yields
  `Path<M, Option<String>>`, and `Option<String>` implements no `TypedValue`
  (`schema/validation.rs:46-81`), so optional text columns keep `computed`.
- **Optional bools.** An emptied checkbox control takes the field's `blank`
  answer under the record-form rule; a missing answer fails `Panel::build`
  naming the field (`panel/build.rs:471-494`), as for any optional control.
- **Read-only rendering.** The detail page renders the stored spelling through
  the same path a `Select` uses when no option matches it
  (`schema/fields/select.rs:455-463`): `true` or `false`, not a label.

## Alternatives

- **A single derive generating record form, table, and schema from one
  struct.** Removes more repetition but couples list display to form shape. Kept
  as future work behind this additive step.
- **`field` generic over `Display`.** Covers integers and timestamps now but
  adds trait-bound and default design (width defaults, sort/search semantics per
  type) this step does not need. Deferred.
- **A `field` that takes the closure after all**
  (`field(lens, |u| u.name.clone())`). Compiles today, but keeps the closure the
  constructor exists to remove, so it buys only the name.

## Open questions

- Blocking-acceptance: none for `TextInput::typed` and `Select::checkbox`.
  `TextColumn::field` is blocked on the upstream API in the Blocker section, so
  no part of it merges before that API does.
- Blocking-implementation: a test pins the duplicate-key rule (last pair wins)
  before the checkbox merges. `panel/forms/decode.rs:367-369` implements it and
  no test states it.
- Deferrable: the generic-`Display` overload, and a checkbox `Filter`.

## Out of scope

`RecordForm` derivation, completion, and the `Table` key constructor (#384) are
unchanged. No checkbox `Filter` is added. The guide's `TextColumn::for` and
`TextInput::typed` examples (`docs/guide/src/tables.md:12`,
`docs/guide/src/forms.md:131`) are updated by the implementation PR.
