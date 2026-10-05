# Testing and benchmarks

## Testing a panel

Test a panel over HTTP, in memory: build the router against a test database and send it requests.
`TestClient` carries cookies, a tenant and a CSRF token, with helpers to build form bodies and read
responses. An app reaches it as `tablo::testing` through the facade's `testing` feature; enable it
for tests only:

```toml
[dev-dependencies]
tablo = { version = "0.1", features = ["sqlite", "testing"] }
```

```rust
use tablo::testing::{TestClient, form_body};

{{#include ../../../examples/guide/tests/it.rs:testing-no-delete}}
```

- **Cover every ability your policy decides** with a request it allows and one it refuses, and
  assert on the database as well as the status code.
- **Cover `query()` scoping** by seeding a row the scope excludes and asserting that the list,
  the detail page and a delete all miss it.
- **Tenancy.** `client.tenant(id)` scopes a request to a tenant, the way a signed-in user's tenant
  would.
- **Signed-in requests.** With authentication on, sign in through `POST /admin/login` once and
  reuse the client's cookies, or insert an `AuthSession` row directly to skip the password hash.
- **Declaration mistakes.** A panel that refuses to mount returns a `MountError` inside the
  router builder's error. Downcast to it and match on each mistake's `DeclarationErrorKind`
  rather than on its message:

```rust
{{#include ../../../examples/guide/tests/it.rs:testing-mount-error}}
```

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
