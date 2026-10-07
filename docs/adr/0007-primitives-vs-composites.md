# 0007 `primitives/` is synced, `composites/` is owned

`tablo-ui/src/components/primitives/` is a verbatim copy of `topcoat-ui-registry`, written by
`cargo xtask sync-topcoat-ui` under a `SYNC` header with the registry version and content hash;
the xtask guards fail on drift. The vendored set is `xtask::VENDORED_PRIMITIVES` and must be
closed under the registry's dependencies. `composites/` holds the components Tablo writes, which
never sync. One crate re-exports both.

Styling stays per app: the app owns `styles.css` and its `tablo_build::tailwind()` build, and
edits tokens to customize. There is no `Panel::theme`.

## Rejected

- Two crates, one per half: crate proliferation.
- Copying the sources into each app: upgrades break them.
