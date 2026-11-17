# tablo-ui

The styled components `tablo-core` renders, as Topcoat functions: buttons, tables, dialogs,
sidebars, form fields and the rest, with the scripts they need exposed as `Asset` constants. Apps
depend on this crate instead of running `topcoat ui add`.

```sh
cargo add tablo-ui
```

`components/primitives/` mirrors
[`topcoat-ui-registry`](https://crates.io/crates/topcoat-ui-registry); `components/composites/`
holds the components Tablo owns, the ones that carry its look among them: alert, badge, button,
card, checkbox, input, radio group, select, switch, table and textarea keep the registry's names
and props, drawn in Tablo's classes. The facade re-exports the crate as `tablo::ui`.

The public API is the composites plus the primitives a custom page builds from: field, label,
separator and skeleton. The other primitives (dialogs, menus, the sidebar, pagination) are hidden:
`tablo-core` renders with them, but they follow the registry rather than Tablo's semver. The
[API reference](https://docs.rs/tablo-ui) lists the public ones.
