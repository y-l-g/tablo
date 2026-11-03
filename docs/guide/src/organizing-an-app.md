# Organizing an app

[Your first panel](./first-panel.md) fits in one file, and an app with a few resources can stay
that way. This chapter is for the app that outgrows it: where its models, resources, panels and
settings go, and which modules import which. Tablo checks none of it. Mounting checks what you
declare, wherever it lives (see [Mount errors](./mount-errors.md)), so these are conventions,
not rules.

The [Tablo demo](https://github.com/y-l-g/tablo-demo) follows this layout: a shop, a blog, HR and
projects, served by an `admin` panel for staff and an `app` panel customers sign up on. Its paths
are this chapter's examples.

## Modules by domain

Group the code by the part of the business it serves, not by the kind of item it is:

```text
src/
├── main.rs         reads the configuration, connects, seeds, serves the router
├── lib.rs          the modules, and models(): every model the database maps
├── config.rs       every environment variable, read once
├── router.rs       mounts the panels on one router
├── panels/         one function per panel, and what the panels share
├── access.rs       what each role may do in each domain
├── shop/
│   ├── mod.rs      the names the panels and other domains use
│   ├── models.rs   the domain's Toasty models and their enums
│   ├── brand.rs    one resource: its definition, form and record functions
│   └── order/      a resource that outgrew its file
│       ├── mod.rs
│       ├── actions.rs
│       ├── items.rs
│       └── pipeline.rs
├── blog/
├── hr/
├── shared/         what two domains or more use
└── seed/           the demo data
```

A change to orders then touches `shop/order/`. Split by kind, into `models/`, `resources/` and
`actions/`, the same change touches three directories, and none of them says what belongs to
orders.

## A domain module

- **`models.rs`** holds the domain's Toasty models together. A relation is declared on both of
  its models (`has_many` on one, `belongs_to` on the other), so the two sides sit in one file. A
  model may name another domain's model: `shop` products carry `blog` comments.
- **One file per resource** holds the resource, its `RecordForm`, its record functions and its
  actions. The resource's definition and the form it renders change together.
- **`mod.rs`** declares the files privately and re-exports what the panels and the other domains
  name: the resources, the models, a widget. Its doc comment says what the domain holds and which
  sidebar group its resources form.

A resource becomes a directory when one file no longer reads well: its actions move to
`actions.rs`, a widget to its own file, and the resources its detail page shows as
[related tables](./detail-pages.md#related-tables) next to it. Those child resources call
`ResourceDef::without_navigation()`, so they live with their owner rather than in the sidebar:
`shop/order/items.rs` holds an order's lines.

## Panels

`panels/` has one function per panel that returns its `Panel`, and a function for what every
panel shares, such as the brand, the locales and the translations:

```rust
pub fn admin_panel(config: &Config) -> Panel {
    shared(Panel::new("admin"))
        .auth(Auth::custom(DemoAuth))
        .resource::<shop::ProductResource>()
        .resource::<shop::OrderResource>()
        // …
        .resource::<hr::EmployeeResource>()
        // …
}
```

The panel picks its resources, and a resource never names a panel: a resource registered by
[several panels](./panel-and-routing.md#several-panels) is declared once. Register a domain's
resources together. Entries of equal
[`navigation_order`](https://y-l.fr/tablo/nightly/api/tablo/struct.ResourceDef.html#method.navigation_order)
keep their registration order, and a sidebar group sits where its first entry lands, so the
registration order is the sidebar's.

`router.rs` builds the router: the database, the cookie key, mail, assets and each panel. `main`
only reads the configuration, connects and serves.

## Access

Put the policies in one module, `access.rs`, and give each resource its policy from there:

```rust
ResourceDef::new()
    .navigation_group(tablo::text!("navigation-shop"))
    .policy(access::policy(Domain::Shop))
```

One function answers every ability of every resource of a domain from the user's role, so which
role reaches what is one table rather than a closure per resource. A page asks the same module
through `PageDef::access`: the shop dashboard opens to `access::reaches(cx, Domain::Shop)`.
[Building a policy](./policy-auth-tenancy.md#building-a-policy) covers the policies themselves.

## Configuration

Read the environment in one place. `config.rs` holds a `Config` struct that `main` builds once, at
start, and passes by reference to the router and the panels. A variable set to a value the app
cannot use then fails at start, not on the first request that reads it, and `config.rs` lists
every variable the app reads.

## Imports go one way

```text
main → router → panels → domains → shared
seed → domains
```

- Domains import `shared` and each other's models, never a panel, the router or the seed.
- `shared` imports no domain. Code that one domain uses belongs in that domain, so `shared` holds
  only what two or more use.
- The seed imports the domains, and nothing imports the seed but `main`.

A domain that imported a panel would tie its resources to that panel. Keeping the arrows one way
also means a domain reads on its own: its directory holds everything a resource does, and the
panel only says where it appears.

## Translations

Each panel loads one Fluent catalog per locale with `Panel::translations`, so the app keeps one
file per locale under `locales/` and includes it with `include_str!`. Panels that share their
words load the same catalogs from the shared panel function. Tablo derives a label's key from
what it labels (`field-…`, `option-…`), so those keys follow the models rather than the files;
[Labels](./translations.md#labels) lists them.
