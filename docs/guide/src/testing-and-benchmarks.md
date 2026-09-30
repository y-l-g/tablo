# Testing and benchmarks

## Testing a panel

Test a panel over HTTP, in memory: build the router against a test database and send it requests.
The `tablo-test` crate provides `TestClient`, which carries cookies, a tenant and a CSRF token, and
helpers to build form bodies and read responses. It is not published; depend on it from the same
source as `tablo-core`.

```rust
use tablo_test::{TestClient, form_body};

#[tokio::test]
async fn books_cannot_be_deleted() {
    let db = seeded_db().await; // your fixture: an in-memory database with rows
    let id = first_book_id(&db).await;
    let router = Panel::new("admin")
        .app_context(db.clone())
        .resource::<BookResource>()
        .auth(Auth::disabled())
        .build()
        .unwrap();
    let client = TestClient::new(&router);

    assert_eq!(client.get("/admin/books").await.status(), 200);

    // A POST needs the CSRF cookie and a matching `csrf_token` field.
    let token = uuid::Uuid::new_v4().to_string();
    let response = client
        .csrf(&token)
        .post_form(
            &format!("/admin/books/{id}/delete"),
            form_body(&[("csrf_token", &token), ("confirm", "1")]),
        )
        .await;
    assert_eq!(response.status(), 403); // `can_delete_any` is not overridden
}
```

- **Cover every policy predicate** with a request that it allows and one that it refuses, and
  assert on the database as well as the status code.
- **Cover `query()` scoping** by seeding a row the scope excludes and asserting that the list,
  the detail page and a delete all miss it.
- **Tenancy.** `client.tenant(id)` scopes a request to a tenant, the way a signed-in user's tenant
  would.
- **Signed-in requests.** With authentication on, sign in through `POST /admin/login` once and
  reuse the client's cookies, or insert an `AuthSession` row directly to skip the password hash.

`examples/showcase/tests/` is a complete suite covering lists, forms, deletes, filters, export,
tenancy, uploads and authentication; its `common` module holds the fixtures above.

## The browser scripts

`tablo-ui`'s client scripts are plain browser scripts with no build step. Their unit tests run on
Node's built-in runner:

```sh
node --test crates/tablo-ui/assets/*.test.js
```

## Benchmarks

The benchmark renders a 50-row list with two included relations, under tenancy and policy, through
the same path as the panel's list page:

```sh
cargo run --manifest-path benchmarks/tablo/Cargo.toml -- --bench
./benchmarks/scripts/bench.sh   # adds an HTTP run with `oha`
```

The target is under 40 ms at the median on local SQLite. It is a reference, not a pass/fail gate:
the harness prints the target beside the measurement. Results are written to
`benchmarks/results/`, which is not committed. `benchmarks/README.md` covers the setup, the
optional PostgreSQL run and the method.
