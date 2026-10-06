# Embedded values: a derived codec, and the discriminant column as the variant rule

Date: 2026-09-22 — Status: accepted

## Decision

**1. The codec derives from the type shape; keys come from the schema node.**
`#[derive(EmbeddedForm)]` builds one schema node per value through a hidden builder: one resolved
`Field` per leaf, a nested node per `#[form(embed)]` field, and for an enum the variant control,
the `#[shared(..)]` columns once, and one group per variant. The node holds its keys; the app
surface is `EmbeddedForm::{write_form, read_form}` plus the generated `form`. Each call builds the
node once from the request app schema. A field is embedded only when marked `#[form(embed)]`;
every other field is a scalar asserting `FormScalar`, so an app `TypedValue` is a leaf.

**2. A field is a leaf or a value.** A scalar leaf answers blank like a record-form field
(ADR-0022): `#[form(blank = ..)]`, `""` for an `#[form(optional)]` `String`, `None` for an
`Option<T>`, `false` for a `bool`; a leaf with none is required and refuses its key inline.
Per-field overrides are `#[form(label = "…")]`, `#[form(multiline = N)]`, `#[form(blank = ..)]`,
and `#[form(optional)]`; an unknown key rejects at compile time.

**3. The variant is the discriminant column.** `write_form` writes the discriminant and the active
variant's leaves; `read_form` returns `Result` and reads the variant from the submitted
discriminant in this order: a named discriminant always wins, and an undeclared one is a
`FieldError` on its key; with no discriminant named at all, the first variant in declaration
order with a payload of its own submitted (a `#[shared(..)]` column never selects); otherwise
the first variant.

**4. The variant control is a `Select` over the discriminant.** It renders one option per declared
variant; each payload sits in its group, and a runtime signal holding the chosen variant hides
the other groups. Every group still submits, and the server parses the chosen one; with
JavaScript off only the stored variant's group shows. A create form opens on the empty choice,
deliberately not `required`, so an empty submit reaches the payload fallback; with JavaScript off
it shows no group. A `#[shared(..)]` column renders once outside
every group; a unit variant gets its group too.

**5. Hydration is the record form's.** A record form binds the whole value with
`#[record_form(embed)]` (ADR-0022); the edit handler decides which keys posted. A leaf of a
variant group the discriminant hides is not read. `any_present` serves an enum payload fallback
over a nested value.

**6. Boundaries.** A `#[document]` inside a value, a relation inside one, an `Option` of a nested
value, an enum nested inside an enum variant, and tuple or unit structs stay unsupported. Labels
default to the humanized field name and accept overrides; `Schema::extend` serves a derived form
with more controls than a tuple holds. The shared tuple ceiling is eight for `IntoSchema`,
`IntoColumns`, and `IntoFilters`.
