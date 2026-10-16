# 0007 `primitives/` is synced, `composites/` is owned

`tablo-ui/src/components/primitives/` is a verbatim copy of `topcoat-ui-registry`, written by
`cargo xtask sync-topcoat-ui` under a `SYNC` header with the registry version and content hash;
the xtask guards fail on drift. The vendored set is `xtask::VENDORED_PRIMITIVES` and must be
closed under the registry's dependencies. `composites/` holds the components Tablo writes, which
never sync. One crate re-exports both.

Only the primitives listed in `xtask::PUBLIC_PRIMITIVES`, the ones a custom page builds from, are
public API; the sync declares the others `#[doc(hidden)]` and the crate re-exports them hidden. A
registry change to a hidden primitive reaches `tablo-core` without moving Tablo's semver.

Styling stays per app: the app owns `styles.css` and its `tablo_build::tailwind()` build. The
default tokens ship in `tablo-build`, which imports them after Tailwind and before the app's file,
so `styles.css` holds only the tokens the app redeclares. There is no `Panel::theme`.

## Rejected

- Two crates, one per half: crate proliferation.
- Copying the sources into each app: upgrades break them.
- Copying the default tokens into each app's `styles.css`: the same, for the theme.
- Publishing every vendored primitive: each registry sync could force a Tablo semver bump for
  components no app calls.
