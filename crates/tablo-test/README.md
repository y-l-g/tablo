# tablo-test

Drives a built Topcoat `Router` without a socket, carrying the session cookie, the CSRF token and
the tenant, and reads values back out of its responses: bodies, cookies, form fields, and the
semantic HTML queries over table rows, field errors and filter options.

The `tablo` facade re-exports the crate as `tablo::testing` behind the `testing` feature, which is
how a suite uses it:

```sh
cargo add --dev tablo --features testing
```

```rust
use tablo::testing::TestClient;

let client = TestClient::new(&router);
let response = client.get("/admin/books").await;
assert_eq!(response.status(), 200);
```

The [testing chapter](https://y-l.fr/tablo/nightly/guide/testing-and-benchmarks.html) covers the
suite layout.
