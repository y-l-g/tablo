# tablo-ui

The styled components `tablo-core` renders, as Topcoat functions: buttons, tables, dialogs,
sidebars, form fields and the rest, with the scripts they need exposed as `Asset` constants. Apps
depend on this crate instead of running `topcoat ui add`.

```sh
cargo add tablo-ui
```

`components/primitives/` mirrors
[`topcoat-ui-registry`](https://crates.io/crates/topcoat-ui-registry); `components/composites/`
holds the components Tablo owns. The facade re-exports the crate as `tablo::ui`.

The public API is the composites plus the primitives a custom page builds from: alert, badge,
button, card, checkbox, field, input, label, select, separator, skeleton, table and textarea. The
other primitives (dialogs, menus, the sidebar, pagination) are hidden: `tablo-core` renders with
them, but they follow the registry rather than Tablo's semver. The
[API reference](https://docs.rs/tablo-ui) lists the public ones.
