# Architecture Decision Records

An ADR records one decision and the reasoning needed to work with the code. Records are amended
in place: the Decision states the current rule. Where code and an ADR disagree, the code wins and
the ADR is fixed. Behaviour lives in the [user guide](../guide/src/introduction.md) and in
rustdoc; an ADR keeps only the decision, the rejected alternatives, and the constraint future
code must respect.

| ADR | Decision |
| --- | --- |
| [0001](0001-typed-field-lenses.md) | Fields bind through typed lenses, never string state paths |
| [0002](0002-query-seam.md) | `Resource::query` scopes rows; the framework owns tenancy |
| [0004](0004-action-auth.md) | Mutations are transactional and per-record, with a post-commit hook |
| [0007](0007-primitives-vs-composites.md) | `primitives/` is synced, `composites/` is hand-written; Tailwind stays per app |
| [0008](0008-panel-declarative-resources.md) | Panel declares resources and owns the shell document |
| [0011](0011-relations-via-resource-query.md) | Relations use explicit includes, not a `Relation` trait |
| [0013](0013-panel-auth.md) | Authentication is part of the panel behind one override seam |
| [0017](0017-media-uploads.md) | Uploads go through an app-level `Uploader` |
| [0018](0018-export-include-scoping.md) | Columns declare typed includes; `query` scopes rows, `view_query` feeds the detail page |
| [0019](0019-embedded-values.md) | Embedded values derive their codec; the discriminant picks the variant |
| [0022](0022-record-forms.md) | A form writes through a derived typed struct, completed from the stored record |
| [0023](0023-resource-definitions.md) | A resource declares one `ResourceDef`; each panel owns the copy it mounts |
| [0024](0024-core-layers.md) | `tablo-core`'s modules form four layers a test enforces |

## Retired

A retired number stays retired and is never reused. Code cites an ADR only for a genuine seam.

| ADR | Status |
| --- | --- |
| 0003 | Retired |
| 0006 | Retired: folded into 0007 |
| 0009 | Retired |
| 0010 | Retired |
| 0012 | Retired: tenancy folds into 0002 |
| 0014 | Retired: see `xtask/tests/it.rs` |
| 0015 | Retired: see `docs/dev/TESTING.md` |
| 0016 | Retired |
| 0020 | Retired |
| 0021 | Retired: showcase-owned |
