# Testing and benchmarks

## Testing a panel

Test a panel over HTTP, in memory: `mount` builds the router against a test database, and
`TestClient` sends it requests. The client carries cookies, a tenant and a CSRF token; `sign_in`
signs it in as a user, `log_in` goes through the login form, and `submit` and `submit_multipart`
post a form with its CSRF pair. The `testing` module also builds form bodies and reads responses
back: the table's `rows` and `row_keys`, why a field was refused (`field_error`), the flash
`notification`, why a table is empty (`empty_table`), and whether a button asks first
(`confirms_first`). An app reaches it as `tablo::testing` through the facade's `testing` feature;
enable it for tests only:

```toml
[dev-dependencies]
tablo = { version = "0.9.0", features = ["sqlite", "testing"] }
```

```rust
use tablo::{auth::create_admin, testing::{TestClient, mount_with_handle}};

{{#include ../../../examples/guide/tests/it.rs:testing-no-delete}}
```

`client.resource::<R>()` sends typed requests to one resource: it loads its pages, builds its
form posts from the form's derived field enum (`.set(BookFormField::Title, "Emma")`), and posts
its deletes and actions. Each URL comes from the panel, so a test follows a renamed slug, fails to
compile on a renamed field or action, and panics naming the resource when the panel serves no
such page instead of asserting on a 404. A submission answers with a `Reply` that reads its
status, redirect, notification, rows and field errors, and `follow()` loads the page a redirect
names. [`ResourceClient`](https://y-l.fr/tablo/nightly/api/tablo_test/struct.ResourceClient.html),
[`Submission`](https://y-l.fr/tablo/nightly/api/tablo_test/struct.Submission.html) and [`Reply`](https://y-l.fr/tablo/nightly/api/tablo_test/struct.Reply.html) list the methods.

The free functions remain for what the typed client does not cover: a raw body, a request to a
route the panel does not mount, a header to assert.

- **Cover every ability your policy decides** with a request it allows and one it refuses, and
  assert on the database as well as the status code.
- **Cover `query()` scoping** by seeding a row the scope excludes and asserting that the list,
  the detail page and a delete all miss it.
- **Tenancy.** `client.tenant(id)` scopes a request to a tenant, the way a signed-in user's tenant
  would.
- **Signed-in requests.** `client.sign_in(&db, "admin", &user)` signs the client in as `user`
  with no login form and no password hash
  ([`TestClient::sign_in`](https://y-l.fr/tablo/nightly/api/tablo_test/struct.TestClient.html#method.sign_in)).
- **Declaration mistakes.** A panel that refuses to mount returns a `MountError` inside the
  router builder's error. `refusal` lists its mistakes; match on each one's
  `DeclarationErrorKind` rather than on its message:

```rust
{{#include ../../../examples/guide/tests/it.rs:testing-mount-error}}
```

`examples/showcase/tests/` is a complete suite covering lists, forms, deletes, filters, export,
tenancy, uploads and authentication; its `common` module holds the fixtures above.

## The browser scripts

`tablo-ui`'s searchable-select script is a plain browser script with no build step. Its unit
tests run on Node's built-in runner:

```sh
node --test crates/tablo-ui/assets/selects.test.js
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
