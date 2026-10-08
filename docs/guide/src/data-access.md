# Data access

The panel loads rows on its own pages. This chapter covers querying Toasty from your code: a
[page](./panel-and-routing.md#pages), a record function, or a public page. The
[Toasty guide](https://tokio-rs.github.io/toasty/0.11.0/guide/) covers the query API in full.

## Getting the database

`app_context(db)` registers the `Db` on the app's router, where every panel mounted on it reads it;
`tablo_core::db::db(cx)` returns it. Cloning a
`Db` is cheap, and statements take it by `&mut`:

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

`wired_table` is the table the request's panel mounted for the resource, with the row actions
its policy allows; it returns an error when that panel does not mount the resource. `render`
loads the page of rows the table's state selects and renders it live, as the resource's list is:
the table keeps its state in signals the page reads, so a change reruns your page in place. Its
links point at your page's own URL. A page that renders two tables gives each a prefix with
`.prefixed("posts")`, which spells its parameters `posts.q`, `posts.sort`, and so on.

## Outside a request

`scoped_query`, `can`, `write_create` and the other entry points that answer from a mounted def
read the resources a panel mounts, so code that runs outside a request, such as a background job,
reads them from the panel the router mounts. After `.panel(..)`, `panel_handle("admin")` returns
that panel's `PanelHandle`, which holds the router's database and the panel's mounted resources and
is cheap to clone into a job:

```rust
{{#include ../../../examples/guide/src/data_access.rs:data-access-panel}}
```

`admin.context()` returns a context holding the database and those resources, with no request,
session or tenant, without declaring the panel again. Add a tenant with `.with(Tenant(id))`:

```rust
{{#include ../../../examples/guide/src/data_access.rs:data-access-job}}
```

A context is one unit of work, as a request is: the loads it memoizes stay cached while it lives,
so build one per job run. It carries no signed-in user, so a policy that reads one answers for a
signed-out request. A context with no panel at all knows no resource: every entry point that reads
a mounted def answers as for a resource the panel does not mount, and `write_update` writes without
consulting the context.

`Panel::context(&db)` builds the same context from a panel no router mounts, such as a test's. It
declares and checks the panel's resources on every call, refusing the declaration errors
`.panel(..)` refuses.

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

A table updates through a rerun of its page, which runs the page's layers and guards like any
request. Topcoat serves a shard at its own runtime path, where no page guard runs: a shard you
write starts with `auth::guard(cx)?`, and checks `can_list::<R>(cx)` before it lists `R`'s rows.
