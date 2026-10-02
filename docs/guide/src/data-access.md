# Data access

The panel loads rows on its own pages. This chapter covers querying Toasty from your code: a
[page](./panel-and-routing.md#pages), a record function, or a public page. The
[Toasty guide](https://tokio-rs.github.io/toasty/0.10.0/guide/) covers the query API in full.

## Getting the database

`app_context(db)` registers the `Db` on the app's router, where every panel mounted on it reads it;
`tablo_core::db::db(cx)` returns it. Cloning a
`Db` is cheap, and statements take it by `&mut`:

```rust
let mut db = tablo_core::db::db(cx);
let users = User::all().exec(&mut db).await?;
```

Inside a record function, run statements through the transaction you were handed (`ex`), never
through a second handle: a single-connection pool such as `sqlite::memory:` would wait forever for
the connection the transaction holds.

## Querying

```rust
User::filter(User::fields().email().eq("ada@example.com"))
User::filter(User::fields().name().starts_with(prefix)).order_by(User::fields().name().asc())
```

Toasty binds values as parameters. If you build a `LIKE` pattern from user input, escape `%`, `_`
and your escape character first and pass it with `like_with_escape`, as the table search does.

**Load a resource's rows through `scoped_query`.** `scoped_query::<PostResource>(cx)?` is the
resource's `query()` with its tenant scope applied, and answers 403 when the request has no
tenant. `PostResource::query(cx)` has no tenant scope. See
[Tenancy](./policy-auth-tenancy.md#tenancy).

A public page has no resource behind it, so it states its own filters, tenant included:

```rust
let posts = Post::filter(Post::fields().status().eq("published".to_string()))
    .include(Post::fields().author())
    .exec(&mut db)
    .await?;
```

## Relations

Toasty loads a relation only when the query includes it. Include every relation you read, in
the same query:

```rust
let posts = Post::all().include(Post::fields().author()).exec(&mut db).await?;
for post in &posts {
    let name = &post.author.get().name; // no extra query
}
```

Reading a relation that was not included panics in `get()`; check `is_unloaded()` first where a
missing include is possible. In a table column, declare the relation with `TextColumn::include`
instead: see [Tables](./tables.md#columns).

## A resource's table on your own page

A page can render a resource's list table over its own query — here, only featured posts — with
the same columns, filters and row actions as the resource's list:

```rust
let table = tablo_core::panel::wired_table::<PostResource>(cx);
let state = TableState::from_cx(cx);
let query = scoped_query::<PostResource>(cx)?.filter(Post::fields().featured().eq(true));
let page = TablePage::load(cx, &table, query, &state).await?;
let body = table.render_with_state(cx, page, &state, "/admin/featured").await?;
```

`wired_table` adds the row actions the resource's policy allows. The last argument of
`render_with_state` is the URL the table's search, sort and pager links point at: the page's own.

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

A table with `live_search()` refreshes through a Topcoat shard request. Page and layout guards do
not run for shard requests, so the panel's shard checks authentication, the tenant and
`ViewAny` itself; a shard you write starts with `auth::guard(cx)?`, and checks
`can_list::<R>(cx)` before it lists `R`'s rows.
