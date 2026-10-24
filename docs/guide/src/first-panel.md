# Your first panel

This chapter builds a complete admin in one file: a `Book` model, a resource for it, and the
`main` that serves it with a login page. `examples/quickstart` is this app with writes opened and the
stylesheet wired in.

## Dependencies

An app depends on the `tablo` facade, picks its database driver with a `tablo` feature, and builds
its stylesheet with `tablo-build`. It names `topcoat` and `toasty` directly for their macros, at the
versions Tablo uses:

```toml
[dependencies]
tablo = { version = "0.8.0", features = ["sqlite"] }
{{#include ../../../examples/quickstart/Cargo.toml:upstream}}

[build-dependencies]
tablo-build = "0.8.0"
```

The toolkit enables no driver itself: `sqlite`, `postgresql`, `mysql` and `turso` each turn on
Toasty's driver of the same name. The `[workspace.dependencies]` of Tablo's `Cargo.toml` hold the
pinned versions. Besides `tablo-build`, which only `build.rs` names, `tablo` is the only Tablo
dependency the app names: every path in this guide starts with `tablo::`, and the derives work
through it.

## The app

```rust
{{#include ../../../examples/guide/src/first_panel.rs:book-app}}
```

Run it and sign in as `admin@example.com` / `secret`:

```sh
cargo run
# open http://127.0.0.1:3000/admin/books
```

## What each part does

- **`tablo::ModelColumns`** gives `Book` its default table and detail page: a column per field a
  person reads, so `id`, which Toasty generates, is left out. See [Tables](./tables.md).
- **`tablo::ModelForm`** writes `BookForm`, the **record form** a submission parses into: one
  field per model field a person fills, so `id` is left out. The record form also lays out the
  form. See [Forms](./forms.md).
- **`impl Resource`** declares the panel's pages for one model. A resource must name `Model` and
  `Form`; every other item has a default, and the default policy denies everything. See
  [Resources](./resources.md).
- **`tablo::models!(..)`** lists every model Toasty maps: the app's, then the ones Tablo ships,
  the built-in login's users and sessions among them. Built with `toasty::models!` instead, a `Db`
  missing one fails the mount with an error naming it.
- **`db.push_schema()`** creates the tables, which suits a prototype. A production app runs
  `toasty-cli` migrations instead.
- **`Router::builder().discover()`** starts the app's own router: the app owns it, and the panel
  is one part of it. **`app_context(db)`** gives it the database every handler reads.
- **`Panel::new("admin")`** is a panel at `/admin`, framed in the shell — sidebar, topbar,
  toasts — by a layout it registers itself.
- **`resource::<BookResource>()`** registers the resource's routes under `/admin/books`, adds its
  sidebar entry, and, because the panel has no home page, makes `/admin` redirect to the book
  list. [Panel and routing](./panel-and-routing.md) lists every route.
- **`.panel(..)`** checks every declaration and mounts the panel, or returns a `MountError`
  listing every mistake it found. **`build()`** returns the `Router`.
- **`topcoat::start`** serves the router on `HOST`:`PORT` (default `127.0.0.1:3000`) until Ctrl+C
  or `SIGTERM`.

## What the example leaves out

- **Writes.** `ReadOnly` allows only listing and viewing, so the create link is hidden, the create
  and edit pages answer 403, and rows show no Edit or Delete action.
  [Policy, auth, tenancy](./policy-auth-tenancy.md) opens them.
- **Styling.** The example registers no asset bundle, so pages render unstyled and without the
  shell's scripts, but every page and form works. The router's `.assets(..)` and the panel's
  `shell_assets` add the stylesheet, the font and the scripts; see
  [Panel and routing](./panel-and-routing.md#assets). The stylesheet itself comes from the build
  script below.

## The stylesheet

The panel's markup carries Tailwind classes, so the app generates a stylesheet that covers them.
The app owns `styles.css` at its package root. The build imports Tailwind and Tablo's default
theme ahead of it, so the file names the app's own sources and redeclares only the tokens it
changes:

```css
@source "./src/**/*.rs";

:root {
  --primary: oklch(0.45 0.18 265);
}
```

[Theme tokens](./panel-and-routing.md#theme-tokens) lists every token; each is declared under
`:root` and again under `.dark`, in `crates/tablo-build/src/theme.css`.

`build.rs` runs the build:

```rust
fn main() {
{{#include ../../../examples/guide/src/build_script.rs:build-script}}
}
```

`tablo_build::tailwind()` adds Tablo's own sources to the build, wherever Cargo unpacked them, and
writes `$OUT_DIR/tailwind.css`, which `tailwind::stylesheet!()` hands to `Panel::shell_assets`. The
stylesheet names no path into Tablo. The build downloads the Tailwind CLI on first run.
