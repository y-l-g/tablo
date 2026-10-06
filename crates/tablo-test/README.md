# tablo-test

Drives a built Topcoat `Router` without a socket, carrying the session cookie, the CSRF token and
the tenant. The body and HTML helpers it uses live in `tablo_core::protocol` and are re-exported
here.

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
