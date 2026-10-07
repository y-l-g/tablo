# Architecture Decision Records

One record per decision: the decision, the constraint code must respect, and the alternatives it
rejected. Behavior lives in rustdoc and the guide. Records are amended in place; when code and a
record disagree, the code wins and the record is fixed. Missing numbers are retired, never reused.

| ADR | Decision |
| --- | --- |
| [0001](0001-typed-field-lenses.md) | Fields and columns bind through typed lenses, never string paths |
| [0002](0002-query-seam.md) | `Resource::query` scopes rows; the framework owns tenancy on a typed column |
| [0004](0004-action-auth.md) | Mutations are transactional and per-record, with a post-commit hook |
| [0007](0007-primitives-vs-composites.md) | `primitives/` is synced, `composites/` is owned; styling stays per app |
| [0008](0008-panel-declarative-resources.md) | The panel declares resources and owns the shell |
| [0013](0013-panel-auth.md) | Authentication is part of the panel, behind one seam |
| [0017](0017-media-uploads.md) | Uploads go through an app-level `Uploader` |
| [0018](0018-export-include-scoping.md) | Relations are includes declared where they are read |
| [0022](0022-record-forms.md) | Forms write through a derived typed struct, embedded values included |
| [0023](0023-resource-definitions.md) | A resource declares one value; each panel owns what it mounts |
| [0024](0024-core-layers.md) | `tablo-core` is layered, and a test enforces it |
| [0026](0026-topcoat-runtime-only.md) | Topcoat's runtime is the browser layer |
