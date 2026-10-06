# tablo-ui

The styled components `tablo-core` renders, as Topcoat functions: buttons, tables, dialogs,
sidebars, form fields and the rest, with the scripts they need exposed as `Asset` constants. Apps
depend on this crate instead of running `topcoat ui add`.

```sh
cargo add tablo-ui
```

`components/primitives/` mirrors
[`topcoat-ui-registry`](https://crates.io/crates/topcoat-ui-registry); `components/composites/`
holds the components Tablo owns. The facade re-exports the crate as `tablo::ui`; the
[API reference](https://docs.rs/tablo-ui) lists every component.
