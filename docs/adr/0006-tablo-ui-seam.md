# Beautiful primitives in tablo-ui, with the Tailwind seam per app

Date: 2026-08-28 — Status: accepted — Amended: 2026-09-10, 2026-09-16, 2026-09-22, 2026-10-01

## Decision

Tablo must be beautiful by default — a new project that defines a Panel and a Resource gets a
Filament-grade dashboard with no extra setup — yet Topcoat UI is copy-source (`topcoat ui init` +
`topcoat ui add` drops owned files into the app). The beautiful primitives live in the
`tablo-ui` crate, which depends on `topcoat-ui-registry` as a library and re-exports styled
`#[component]`s (table, card, input, label, button, pagination, skeleton, dialog, separator, sheet,
sidebar and the rest). `tablo-core`'s `Table`, `Schema` and `Panel` shell render those components
directly; apps never run `topcoat ui add` and never own component source, so an upgrade cannot be
broken by local edits.

The Tailwind seam stays per-app and explicit: `styles.css` (importing `tailwindcss` + neutral tokens
+ `@source` for the app's own `src/**/*.rs`) and a `build.rs` calling `tablo_build::tailwind()`,
plus `tailwind::stylesheet!()` and the Geist font in the layout. That is the mechanism Topcoat
documents, it keeps tree-shaking per app, and it makes token editing the single customization seam:
change `--primary`, `--background`, etc. in `:root`/`.dark`. There is no Rust `Panel::theme`
builder and no per-cell `attrs`; the narrow class seam is `Section::class`, merged via `class!`
against the token classes rather than replacing them.

Tablo's own markup lives in `tablo-core` and `tablo-ui`, whose sources sit wherever Cargo unpacked
them — a git or registry checkout under `~/.cargo`, or a path dependency — so the app's stylesheet
cannot name them. Each crate declares a `links` key and publishes its `src` directory as
build-script metadata, the `tablo` facade forwards both, and `tablo_build::tailwind()` builds a
Tailwind input in `OUT_DIR` that imports the app's `styles.css` by absolute path and adds one
`@source` per published directory. The app's relative `@source` lines keep resolving against the
app, and the build scans exactly the copies of `tablo-core` and `tablo-ui` that Cargo compiles. This stays the mechanism until Topcoat
scans dependency sources natively.

The Sidebar is an upstream `topcoat-ui-registry` component (topcoat#419), vendored into
`primitives/` by `cargo xtask sync-topcoat-ui`. Its `sidebar_menu_button` takes `active` (not
`is_active`) plus `href`/`tooltip` props, the trigger pair and rail carry `@click` handlers, and
`open`/`mobile_open` are runtime expressions (`Signal<bool>`) with the mobile sheet owned by the
component. `Panel::render_shell` binds the signals, seeds `open` from the `sidebar_state` cookie, and
`assets/sidebar.js` keeps the cookie and `Ctrl+B`.

## Consequences

`examples/quickstart` is the smallest complete setup and is built from outside the repository by
`cargo xtask external-check`, which fails when the stylesheet misses classes only Tablo writes;
`examples/showcase` is the full reference. Empty projects follow the docs (one `styles.css`, one
`build.rs`) until a scaffold automates them.
`Panel::layout_shell` owns the document links, the app passes its generated stylesheet and font
handles through `Panel::shell_assets`, and `Panel::assets` owns the loaded bundle. The primitives
sync is version + sha256-guarded by `xtask/tests/it.rs`. Publishing an
`tablo-ui-registry` for `topcoat ui add --registry tablo` is rejected: it reintroduces the
copy-source steps and the upgrade breakage. Embedding a prebuilt stylesheet in `tablo-ui` and
injecting it automatically stays deferred: it hides Tailwind's build, loses per-app tree-shaking,
and may return as an opt-in feature for demos.
