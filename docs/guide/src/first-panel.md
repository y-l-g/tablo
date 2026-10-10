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
tablo = { version = "0.5.1", features = ["sqlite"] }
{{#include ../../../examples/quickstart/Cargo.toml:upstream}}

[build-dependencies]
tablo-build = "0.5.1"
```

The toolkit enables no driver itself: `sqlite`, `postgresql` and `mysql` each turn on Toasty's
driver of the same name. The `[workspace.dependencies]` of Tablo's `Cargo.toml` hold the pinned
versions. `tablo` is the only Tablo dependency the app names: every path in this guide starts with
`tablo::`, and the derives work through it.

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

- **`#[derive(RecordForm)]`** declares what a form submission parses into. Each field is named
  and typed like the model's field, so a renamed column fails to compile. The derive also lays
  out the list's table, the form and the detail page from those fields. See [Forms](./forms.md).
- **`impl Resource`** declares the admin for one model. A resource must name `Model` and `Form`;
  every other item has a default, and the default policy denies everything. See
  [Resources](./resources.md).
- **`Db::builder().models(..)`** lists every model Toasty maps, including the two tables the
  built-in login uses. With authentication on, mounting the panel returns an error naming the
  missing models.
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
The app owns `styles.css` at its package root: it imports `tailwindcss`, declares the theme tokens
(`examples/quickstart/styles.css` is a neutral set to start from), and names its own sources.

```css
@import "tailwindcss";
@source "./src/**/*.rs";
/* the theme tokens: --background, --foreground, --primary, ... */
```

`build.rs` runs the build:

```rust
{{#include ../../../examples/guide/src/build_script.rs:build-script}}
```

`tablo_build::tailwind()` adds Tablo's own sources to the build, wherever Cargo unpacked them, and
writes `$OUT_DIR/tailwind.css`, which `tailwind::stylesheet!()` hands to `Panel::shell_assets`. The
stylesheet names no path into Tablo. The build downloads the Tailwind CLI on first run.
