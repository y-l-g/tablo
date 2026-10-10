# tablo-build

Build-script helper that runs the app's Tailwind build over Tablo's own sources, so the stylesheet
carries the classes only Tablo writes.

```sh
cargo add --build tablo-build
```

In `build.rs`:

```rust
fn main() {
    tablo_build::tailwind().expect("the Tailwind build runs");
}
```

The app owns `styles.css` at its package root; `tailwind()` imports Tailwind and Tablo's default
theme ahead of it, adds Tablo's sources, and writes `$OUT_DIR/tailwind.css`, which `topcoat::tailwind::stylesheet!()` hands to `Panel::shell_assets`.
Those sources come from the `links` metadata of the Tablo crates the package depends on, so `tablo`
(or `tablo-core` and `tablo-ui`) must be under `[dependencies]`. The
[first panel](https://y-l.fr/tablo/nightly/guide/first-panel.html) chapter wires it up.
