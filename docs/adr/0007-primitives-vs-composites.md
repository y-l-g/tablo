# 0007 `primitives/` is synced, `composites/` is owned

`tablo-ui/src/components/primitives/` is a verbatim copy of `topcoat-ui-registry`, written by
`cargo xtask sync-topcoat-ui` under a `SYNC` header with the registry version and content hash;
the xtask guards fail on drift. The vendored set is `xtask::VENDORED_PRIMITIVES` and must be
closed under the registry's dependencies. `composites/` holds the components Tablo writes, which
never sync. One crate re-exports both.

A component that carries Tablo's look is a composite, even where the registry has one: Tablo owns
its button, badge, card, form controls, select, table and alert, under the registry's names and
props, so the look is drawn in Tablo's own classes rather than fought over in a skin. The
registry's copies are not vendored, except where a vendored primitive depends on one (the sidebar
and the pagination on `button` and `input`); those stay hidden.

Only the primitives listed in `xtask::PUBLIC_PRIMITIVES`, the ones a custom page builds from, are
public API; the sync declares the others `#[doc(hidden)]` and the crate re-exports them hidden. A
registry change to a hidden primitive reaches `tablo-core` without moving Tablo's semver.

Styling stays per app: the app owns `styles.css` and its `tablo_build::tailwind()` build. The
default tokens ship in `tablo-build`, which imports them after Tailwind and before the app's file,
so `styles.css` holds only the tokens the app redeclares.

A brand picks one of the themes Tablo ships with `Brand::theme`: a set of tokens for both color
schemes, the sidebar frame it suggests, and a skin. The themes are alternative defaults inside the
same build, selected by `data-tablo-theme` on `<html>` at `:root` specificity, so `styles.css`
still overrides every token, and panels of one app can wear different themes from one stylesheet.
There is no API to register a theme: an app's own look is its `styles.css` over the closest one.
`Brand::sidebar` picks the frame apart from the theme.

Every theme speaks one grammar, written once in `tablo-build`'s `theme/identity.css`: captions,
ruled head bands, the masthead, the notch marking the current page, the highlighter. Its type and
measures are `--tablo-*` properties a theme redeclares. A skin holds what a theme's tokens cannot
say. Both sit in `@layer tablo-skin`, after Tailwind's utilities and before the app's unlayered
rules, and select only the `data-*` hooks the markup already carries, never a utility class: the
owned components carry `data-slot`, and a button its `data-tone` and `data-size`, which a link drawn
as a button repeats. No test asserts appearance, so renaming or dropping a hook means checking each
theme's skin by eye.

An app on the default theme that writes no Tailwind classes of its own may skip that build:
`tablo-ui` ships the same build over Tablo's sources alone, prebuilt, as `DEFAULT_STYLESHEET`. A
showcase test fails when the committed copy drifts from what Tablo's sources build.

## Rejected

- Two crates, one per half: crate proliferation.
- Copying the sources into each app: upgrades break them.
- Copying the default tokens into each app's `styles.css`: the same, for the theme.
- Shipping each theme as a stylesheet the app imports: one theme per app, since one build serves
  every panel, and the sidebar's frame is markup no stylesheet picks.
- Registering an app's own themes: a second styling surface beside `styles.css`.
- Restyling the registry's components from skins alone: every rule fights their utility classes,
  and a registry sync can undo the look without a diff in Tablo.
- Publishing every vendored primitive: each registry sync could force a Tablo semver bump for
  components no app calls.
