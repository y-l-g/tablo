# Architecture Decision Records

An ADR records one decision and the reasoning needed to work with the code. Records are amended
in place: the Decision states the current rule. There is no `Supersedes` field. Where code and an
ADR disagree, the code wins and the ADR is fixed.

| ADR | Decision |
| --- | --- |
| [0001](0001-typed-field-lenses.md) | Fields bind through typed lenses, never string state paths |
| [0002](0002-query-seam.md) | `Resource::query` scopes rows; the framework owns tenancy |
| [0003](0003-reactivity-seam.md) | Suspense, morphing reruns, and one scalar live-search shard |
| [0004](0004-action-auth.md) | Mutations are transactional and per-record, with a post-commit hook |
| [0006](0006-tablo-ui-seam.md) | Styled primitives live in `tablo-ui`; Tailwind stays per app |
| [0007](0007-primitives-vs-composites.md) | `primitives/` is synced, `composites/` is hand-written |
| [0008](0008-panel-declarative-resources.md) | Panel declares resources and owns the shell document |
| [0009](0009-shell-shadcn-parity.md) | The shell is shadcn-shaped, with runtime sidebar state |
| [0010](0010-single-resource-crud.md) | One resource gets working CRUD, policy, and notifications |
| [0011](0011-relations-via-resource-query.md) | Relations use explicit includes, not a `Relation` trait |
| [0012](0012-tenancy-grouping-export.md) | Tenancy rides `Cx`; grouping and CSV export are in-memory |
| [0013](0013-panel-auth.md) | Authentication is part of the panel behind one override seam |
| [0014](0014-shell-js-assets.md) | Shell JS is document-owned, all-loaded, and hook-checked |
| [0015](0015-test-binary-consolidation.md) | One integration-test binary per crate, not per file |
| [0016](0016-detail-pages.md) | Detail pages are one Schema rendered read-only plus relations |
| [0017](0017-media-uploads.md) | Uploads go through an app-level `Uploader` and a clear control |
| [0018](0018-export-include-scoping.md) | Columns declare typed includes; `query` scopes rows, `view_query` feeds the detail page |
| [0019](0019-embedded-values.md) | Embedded values derive their codec; the discriminant picks the variant |
| [0020](0020-confirmed-mutations.md) | A confirmed mutation re-runs the table's shard; the client never morphs the response |
| [0021](0021-media-library.md) | The media library is a `medias` table in the showcase |
| [0022](0022-record-forms.md) | A form writes through a derived typed struct, completed from the stored record |

**Numbers are permanent.** An ADR keeps its number and subject for the life of the repo, because
code comments, vendored headers, and rustdoc cite them. A number is never reused, and a retired
number stays retired.
