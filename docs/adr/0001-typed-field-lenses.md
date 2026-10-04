# Typed field lenses, not string state paths

Date: 2026-08-19 — Status: accepted

## Decision

Every Schema field and Table column binds through a typed Toasty field lens
(`User::fields().email()`), never a string path. The lens carries nullability, uniqueness,
renames, and type. `required` defaults from nullability; a single-segment `String` lens
carries uniqueness from the index list, including composite indexes. Form transport stays
string-keyed (`HashMap<String, String>`); a record form (ADR-0022) parses it into a struct
bound by ident to the model fields. An embedded leaf key resolves at run time. Upstream #115,
#119.

A column renders the value it sorts on, so it needs the field's value off a loaded record as well
as its path, and Toasty reads no value through a path. `lens!(User.name)` writes both halves from
one field name — `User::fields().name()` and `|user| &user.name` — as a `Lens`, so they cannot
name different fields; a chain through a relation does not compile. A row's key is the record's
primary key, read through the model's derived `IntoExpr` (`toasty_compat::pk`), so a table, a
relation and a relationship select declare no key closure.
