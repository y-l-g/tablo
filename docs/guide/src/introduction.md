# Introduction

Tablo is an admin toolkit for Rust. You declare, once per database model, its table, its form and
who may see and change it; Tablo serves the list, create, edit, detail and delete pages for it,
rendered on the server.

Tablo is a layer over two upstream projects:

- [Topcoat](https://github.com/tokio-rs/topcoat) renders the HTML and owns routing, the request
  context `Cx`, reactivity, assets, cookies and sessions. The panel's pages are Topcoat views, and
  your own pages use the same `view!`, `#[component]` and `#[layout]`.
- [Toasty](https://github.com/tokio-rs/toasty) is the ORM. Tablo queries your Toasty models
  directly, so the model is the single source of truth for columns, relations and nullability.

## What you get

- **Server-rendered pages, no SPA.** There is no client build step and no WASM bundle. Search,
  sort, filters and pagination work without JavaScript; the scripts add in-place updates.
- **Typed declarations.** Columns, form fields and filters are built from Toasty field lenses
  such as `User::fields().email()`, so a renamed or retyped column is a compile error.
- **Checks at startup.** Mounting the panel validates every declaration — a form struct that disagrees
  with its schema, two resources on one URL, a tenant-owned resource with no tenant column — and
  returns an error naming the mistake before the server takes a request.
- **Safe defaults.** Every policy predicate denies until you allow it, authentication is on, every
  panel POST verifies a CSRF token, and a tenant-owned resource is scoped to the request's tenant at
  every query.

## What it is not

- **Not a client framework.** Anything that reads the database renders on the server.
- **Not tested across databases.** The test suites and benchmarks run on Toasty's SQLite driver.

## How to read this guide

[Your first panel](./first-panel.md) builds a complete, runnable admin in one file. Each later
chapter covers one part of it:

| Chapter | Covers |
| --- | --- |
| [Panel and routing](./panel-and-routing.md) | the panel builder, the routes it serves, custom and public pages |
| [Resources](./resources.md) | the `Resource` trait: naming, query scoping, writes |
| [Tables](./tables.md) | columns, search, sort, filters, export, live updates, deletes |
| [Forms](./forms.md) | the record form, controls, validation, uploads, embedded values |
| [Detail pages](./detail-pages.md) | the read-only record page and related tables |
| [Policy, auth, tenancy](./policy-auth-tenancy.md) | who may see and change what |
| [Data access](./data-access.md) | querying Toasty from your own pages |
| [Security](./security.md) | the defaults and what your deployment must provide |
| [Testing and benchmarks](./testing-and-benchmarks.md) | testing a panel over HTTP |

The runnable reference is
[`examples/showcase`](https://github.com/y-l-g/tablo/tree/master/examples/showcase): every
feature in this guide is exercised there. Terms are defined in
[`CONTEXT.md`](https://github.com/y-l-g/tablo/blob/master/CONTEXT.md) and design decisions are
recorded in [`docs/adr/`](https://github.com/y-l-g/tablo/tree/master/docs/adr). If this guide
disagrees with the code, the code is right: please open an issue.
