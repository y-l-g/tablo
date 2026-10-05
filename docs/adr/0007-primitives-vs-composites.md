# Primitives vs composites in tablo-ui

Date: 2026-08-28 — Status: accepted

## Decision

`tablo-ui/src/components/` splits in two. `primitives/` mirrors
`topcoat-ui-registry/src/components/` verbatim through `cargo xtask sync-topcoat-ui`; every file
carries a `SYNC` header with version and content hash, and drift fails the guard.
`composites/` holds the hand-written Tablo components — page, error_state, theme, toast — which
compose primitives and tokens and never sync. One crate, `tablo-ui`, re-exports both.

The vendored set is `xtask::VENDORED_PRIMITIVES`; sync, `mod.rs` generation, and verification use
exactly that set. It is the transitive closure of the re-exported components: the guards check
every vendored component's same-registry dependencies against the set and fail by name when one
is missing. Adding a component adds one line plus a sync run.

`sync-topcoat-ui` resolves the registry through `cargo metadata` and reads sources through the
registry API, so synced content matches what Cargo compiles. Topcoat and Toasty pin exact `rev`s.
A two-crate split is rejected as proliferation; per-app copy-source vendoring is rejected for
upgrade breakage.

The Tailwind seam stays per app: the app owns its `styles.css` and the `tablo_build::tailwind()`
build, and token editing is the customization seam. There is no `Panel::theme` builder.
