# Data access

The panel loads rows on its own pages. This chapter covers querying Toasty from your code: a
[page](./panel-and-routing.md#pages), a record function, or a public page. The
[Toasty guide](https://tokio-rs.github.io/toasty/0.11.0/guide/) covers the query API in full.

## Getting the database

`with(db)` registers the `Db` on the app's router, where every panel mounted on it reads it;
`tablo::db::db(cx)?` returns it. Cloning a `Db` is cheap, and statements take it by `&mut`:

```rust
{{#include ../../../examples/guide/src/data_access.rs:data-access-db}}
```

Inside a record function, run statements through the transaction you were handed (`ex`), never
through a second handle: a single-connection pool such as `sqlite::memory:` would wait forever for
the connection the transaction holds.

## Querying

```rust
{{#include ../../../examples/guide/src/data_access.rs:data-access-filters}}
```

Toasty binds values as parameters. If you build a `LIKE` pattern from user input, escape `%`, `_`
and your escape character first and pass it with `like_with_escape`, as the table search does.

**Load a resource's rows through `scoped_query`.** See [Tenancy](./policy-auth-tenancy.md#tenancy).

A public page has no resource behind it, so it states its own filters, tenant included:

```rust
{{#include ../../../examples/guide/src/data_access.rs:data-access-published}}
```

## Relations

Toasty loads a relation only when the query includes it. Include every relation you read, in
the same query:

```rust
{{#include ../../../examples/guide/src/data_access.rs:data-access-relations}}
```

Reading a relation that was not included panics in `get()`; check `is_unloaded()` first where a
missing include is possible. A table or detail column shows a relation with `RelationColumn` or
`CountColumn`, which declare their include: see [Tables](./tables.md#columns).

## A resource's table on your own page

A page can render a resource's list table over its own query — here, only featured posts — with
the same columns, filters and row actions as the resource's list:

```rust
{{#include ../../../examples/guide/src/data_access.rs:data-access-wired-table}}
```

[`wired_table`](https://y-l.fr/tablo/nightly/api/tablo/fn.wired_table.html) is the table the request's panel mounted for the
resource, with the row actions its policy allows, so the page and the list cannot drift apart.
[`WiredTable::render`](https://y-l.fr/tablo/nightly/api/tablo/struct.WiredTable.html#method.render) renders it live, as the resource's
list is: a sort or search reruns your page in place. A page that renders two tables gives each a
prefix with `.prefixed("posts")`.

## Outside a panel

A request outside every panel's prefix, such as a public page, belongs to no panel, however many
the router mounts: `scoped_query` answers it with an error naming the resource. Keep the handle
`panel_handle("admin")` returns (below) on the router, and enter the panel for the request:

```rust
let cx = admin.enter(cx);
let posts = scoped_query::<PostResource>(&cx)?;
```

The entered context answers as the panel mounted its resources, and runs no auth gate:
[`PanelHandle::enter`](https://y-l.fr/tablo/nightly/api/tablo/struct.PanelHandle.html#method.enter) has the details.

## Outside a request

`scoped_query`, `can`, `write_create` and the other entry points that answer from a mounted def
read the resources a panel mounts, so code that runs outside a request, such as a background job,
reads them from the panel the router mounts. After `.panel(..)`, `panel_handle("admin")` returns
that panel's `PanelHandle`, which holds the router's database and the panel's mounted resources and
is cheap to clone into a job:

```rust
{{#include ../../../examples/guide/src/data_access.rs:data-access-panel}}
```

`admin.context()` returns a context holding the database and those resources, without declaring
the panel again. Add a tenant with `.with(Tenant(id))`:

```rust
{{#include ../../../examples/guide/src/data_access.rs:data-access-job}}
```

A context is one unit of work, as a request is, so build one per job run; it carries no
signed-in user. [`PanelHandle::context`](https://y-l.fr/tablo/nightly/api/tablo/struct.PanelHandle.html#method.context) says what it
holds and what a policy sees. `Panel::handle(&db)` builds the same handle from a panel no router
mounts, such as a test's.

## Schema setup

`db.push_schema().await?` creates every registered table, which suits a prototype or a test. A
production app manages its schema with `toasty-cli` migrations.

## Rendering rules

Topcoat may render a page's regions concurrently and re-render them in place, so code that renders
follows three rules:

- **No side effects.** Pages, layouts and components only read. Writes belong in record functions
  or app routes.
- **Deterministic output.** No `HashMap` iteration order, current time or random values in a
  region that re-renders: a re-render must produce the same markup for the same data.
- **Untrusted signals.** A value read on the server with a signal's `get()` or `read()` comes
  from the client. Validate it like any request input.

Share a query between components of one request with Topcoat's `#[memoize]`, and add a Toasty
`#[index]` to columns you filter on.

A table updates through a rerun of its page, which runs the page's layers and checks like any
request. Topcoat serves a shard at its own runtime path, where the page's checks do not run: a
shard you write starts with `auth::guard(cx)?`, and checks `can_list::<R>(cx)` before it lists
`R`'s rows. Declare it on its panel with
[`Panel::endpoint`](https://y-l.fr/tablo/nightly/api/tablo/struct.Panel.html#method.endpoint), which says which panel a shard runs as
otherwise.
