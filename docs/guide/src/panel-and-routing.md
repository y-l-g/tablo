# Panel and routing

`Panel` is the admin application. You configure it with builder calls, and `build()` turns it into
a Topcoat `Router` that serves every registered resource and page, the login page, and the shell
that frames them.

```rust
let router = Panel::new("admin")          // mounted at /admin
    .app_context(db)                      // the toasty::Db every handler uses
    .brand(Brand::new("Acme"))
    .home::<Dashboard>()                  // GET /admin
    .resource::<UserResource>()           // /admin/users and its sub-routes
    .page::<ReportsPage>()                // GET /admin/reports
    .build()?;
```

## Routes

A resource with the slug `users`, on a panel mounted at `/admin`, serves:

| Method | Path | Serves |
| --- | --- | --- |
| `GET` | `/admin/users` | the list |
| `GET` | `/admin/users/{id}` | the [detail page](./detail-pages.md); 404 when the resource declares no `view()` |
| `POST` | `/admin/users/{id}/delete` | delete one record |
| `POST` | `/admin/users/bulk-delete` | delete the selected records |
| `GET` | `/admin/users/export` | the list as CSV |
| `GET`, `POST` | `/admin/users/create` | the create form ¹ |
| `GET`, `POST` | `/admin/users/{id}/edit` | the edit form ¹ |
| `GET` | `/admin/users/options` | option search for a searchable relationship select ¹ |

¹ Only for a resource whose `Form` is a record form; a list-only resource names `NoForm` and gets
none of these. See [Forms](./forms.md).

`{id}` is the record's primary key. Each route checks the resource's policy, so a registered
route is not an open one: see [Policy, auth, tenancy](./policy-auth-tenancy.md).

The panel also serves:

- `GET /admin`: the [home page](#pages), or a redirect to the first registered resource's list
  when there is none.
- `GET`, `POST /admin/login` and `POST /admin/logout`, unless authentication is disabled.

## Builder reference

| Call | Effect | See |
| --- | --- | --- |
| `Panel::new(prefix)` | mounts the panel at `/{prefix}` | |
| `app_context(db)` | installs the `toasty::Db`; `build` fails without it | [Data access](./data-access.md) |
| `resource::<R>()` | registers a resource's routes and sidebar entry | [Resources](./resources.md) |
| `page::<P>()` | registers a page at `/{prefix}/{slug}` | [Pages](#pages) |
| `home::<P>()` | registers a page at `/{prefix}` | [Pages](#pages) |
| `brand(Brand)` | sets the name and optional logo in the sidebar, the topbar and the login card | [Branding](#branding-and-theme) |
| `dark_mode(bool)` | sets the theme a first-time visitor sees | [Branding](#branding-and-theme) |
| `login_hint(text)` | adds a line under the login form | [Branding](#branding-and-theme) |
| `auth(Auth)` | replaces or disables the built-in password login | [Policy, auth, tenancy](./policy-auth-tenancy.md#authentication) |
| `uploads(uploader)` | sets where file fields store their bytes | [Forms](./forms.md#file-uploads) |
| `serve_dir(path, dir)` | serves a directory of files, publicly | [Forms](./forms.md#file-uploads) |
| `assets(bundle)`, `shell_assets(css, font)` | adds the stylesheet, font and scripts | [Assets](#assets) |
| `frame_ancestors(..)`, `without_frame_ancestors()` | changes the anti-framing header | [Security](./security.md) |
| `build()` | checks every declaration and returns the `Router` | |

The builder calls never fail; `build()` returns an error naming what is wrong instead. It refuses a
missing `Db`, two resources or pages with one slug, a slug the panel routes itself (`login`,
`logout`), a slug that is not a single URL segment, a second home page, `shell_assets` without
`assets`, and every resource declaration check described in
[Resources](./resources.md#startup-checks).

## The shell layout

The panel's pages render inside your app's `#[layout]` at the panel prefix. Delegate it to the
panel to get the shell — sidebar, topbar, theme toggle and notification toasts:

```rust
#[layout("/admin")]
async fn admin_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    Panel::layout_shell(cx, slot).await
}
```

## Sidebar

Every resource and page gets one sidebar entry, labelled with its `navigation_label()` and linked
to its list or page URL. Entries sort by `order` (lower first, default `0`); entries with the same
`order` keep registration order, and the home page leads its `order`.

Override `navigation()` on the resource or page to change the order or add an icon. Start from the
default entry so the panel still resolves the URL:

```rust
fn navigation() -> NavigationItem {
    NavigationItem { order: -1, ..NavigationItem::for_resource::<Self>() }
        .icon(tablo_ui::icons::USERS)
}
```

`NavigationItem::for_page::<Self>()` is the equivalent for a page. `NavigationItem::at(label, url)`
links elsewhere; the panel keeps that URL as written. The entry whose URL is the longest match
for the current path is marked active.

## Pages

A page that is not a record list — a dashboard, a report, a settings screen — implements `Page`:

```rust
use tablo_core::{Page, Panel};
use topcoat::{Result, context::Cx, view::{View, view}};

struct ReportsPage;

impl Page for ReportsPage {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! { cx =>
            tablo_ui::page(
                tablo_ui::page_header(tablo_ui::page_title("Reports"))
                tablo_ui::page_content(tablo_ui::card(tablo_ui::card_content("…")))
            )
        })
    }
}

Panel::new("admin").page::<ReportsPage>() // GET /admin/reports
```

- **Slug and label** default to the type name without its `Page` suffix: `ReportsPage` mounts at
  `reports` and is labelled "Reports"; `MediaLibraryPage` mounts at `media-library` and is
  labelled "Media library". Override `slug()` and `navigation_label()` to change them. Pages and
  resources share one slug namespace.
- **The home page.** `Panel::home::<P>()` mounts a page at the prefix itself instead of the
  redirect to the first resource. A panel has at most one; a second fails `build()`.
- **Access.** With authentication on, the page renders only for a signed-in user with panel access,
  inside the shell layout.
- **Forms.** A page serves one `GET`. A form it renders posts to an app `#[route]`; put that route
  under the panel prefix so the auth gate covers it.

The `tablo_ui` composites give a page the same frame as the panel's own pages: `page` sets the
width and padding; `page_header` holds a `page_title`, an optional `page_description` and optional
`page_actions`; `page_content` holds the body. Inside it, use `card` for a panel of content and
`empty_state` for a region with nothing to show. They are styled by the design tokens in your
`styles.css` (`--card`, `--border`, `--primary`, …), so one theme restyles the panel's pages and
yours.

The showcase registers a dashboard as its home page, a media library and a live activity feed.

## Public pages

A page outside the panel prefix is an ordinary Topcoat `#[page]`. The auth gate covers only the
panel prefix and `/_topcoat/runtime`, so such a page is public. `build()` discovers it like any
other Topcoat route; the panel needs no registration call.

`Panel::document` renders the same HTML document as the admin shell — head, assets, theme — around
your own markup:

```rust
// A layout wraps every route under its path: this one wraps /blog and /blog/{id}.
// A layout at "/" would wrap /admin too.
#[layout("/blog")]
async fn blog_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    Panel::document(
        cx,
        "Blog",
        view! { <div class="mx-auto max-w-3xl px-6 py-10">(slot)</div> },
    )
    .await
}

#[page("/blog")]
async fn blog() -> Result<impl View> {
    Ok(view! { "Posts" })
}
```

A public page loads its rows with a plain Toasty query, not through a resource. The resource's
tenant scope applies only to the panel's loaders, so in a multi-tenant app the page states its own
tenant predicate. See [Data access](./data-access.md).

## Branding and theme

```rust
Panel::new("admin")
    .brand(Brand::new("Acme").logo("/logo.svg"))
    .dark_mode(true)
    .login_hint("Demo: admin@example.com / password")
```

- `brand` names the panel in the sidebar header, on the login card, and in the topbar on narrow
  screens. A brand without a logo shows its first letter.
- `dark_mode(true)` makes the panel start dark for a visitor who has not chosen a theme. The theme
  toggle always renders, and a visitor's choice, light or dark, is remembered and wins over this
  default. Without the call the panel starts light.
- `login_hint` renders a muted line under the login form, such as demo credentials.

## Assets

Without assets the panel renders unstyled HTML without the shell's scripts, and every page and
form still works. To style it, register the app's Topcoat asset bundle, then the Tailwind stylesheet and
the font the shell links:

```rust
const GEIST: Font = fontsource_font!(GEIST, host: Asset);

Panel::new("admin")
    .assets(AssetBundle::load().expect("asset bundle"))
    .shell_assets(tailwind::stylesheet!(), GEIST)
```

`shell_assets` also makes the shell load `tablo-ui`'s scripts: live search, confirmation dialogs,
searchable selects, toasts and the sidebar and theme toggles. `build()` refuses `shell_assets`
without `assets`. The stylesheet comes from `tablo_build::tailwind()` in the app's `build.rs`
([Your first panel](./first-panel.md#the-stylesheet)); `examples/quickstart` has the smallest
complete setup and `examples/showcase` the full one.
