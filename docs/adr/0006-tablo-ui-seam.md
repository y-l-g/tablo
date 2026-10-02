# Styled primitives in tablo-ui, with the Tailwind seam per app

Date: 2026-08-28 — Status: accepted

## Decision

A new project defining a `Panel` and a `Resource` gets a working dashboard with no extra setup.
The styled primitives live in `tablo-ui`, which depends on `topcoat-ui-registry` and re-exports
`#[component]`s (table, card, input, label, button, pagination, skeleton, dialog, separator,
sheet, sidebar). `tablo-core` renders those components directly; apps never run `topcoat ui add`
and never own component source.

The Tailwind seam stays per-app: `styles.css` (importing `tailwindcss` plus tokens plus `@source`
for the app's `src/**/*.rs`) and a `build.rs` calling `tablo_build::tailwind()`, plus
`tailwind::stylesheet!()` and the Geist font in the layout. Token editing is the customization
seam: `--primary`, `--background`, and the rest in `:root`/`.dark`. There is no `Panel::theme`
builder and no per-cell `attrs`; `Section::class` merges through `class!`.

`tablo-core` and `tablo-ui` sources live where Cargo unpacks them, so the app stylesheet cannot
name them. Each crate declares a `links` key and publishes its `src` directory as build metadata;
the `tablo` facade forwards both, and `tablo_build::tailwind()` builds an `OUT_DIR` input that
imports the app `styles.css` by absolute path and adds one `@source` per published directory.

The sidebar is the upstream `sidebar` primitive, vendored into `primitives/` by `cargo xtask
sync-topcoat-ui`. `sidebar_menu_button` takes `active` plus `href`/`tooltip`; `open`/`mobile_open`
are `Signal<bool>` runtime expressions with the mobile sheet owned by the component.
`Panel::render_shell` binds the signals, seeds `open` from the `sidebar_state` cookie, and
`assets/sidebar.js` persists the cookie and `Ctrl+B`.
