# Testing

Rules for every test in this repo: Rust integration tests under
`examples/showcase/tests/`, unit tests in `#[cfg(test)]` modules, the xtask
contract tests, and the JavaScript suites under `crates/tablo-ui/assets/`.

## Rules

- Every test protects a specific behavior or catches a plausible bug. Before
  writing it, identify what incorrect behavior would make it fail.
- Do not write tests that only verify hardcoded values. Pin behavior, not copy:
  assert row counts, redirects, database state, and link targets rather than the
  exact wording of a message or button. A passing rename must not break the
  suite.
- Derive expected results from the intended behavior. Do not calculate them by
  repeating the implementation or calling the same code being tested.
- Keep tests sensitive to broken behavior and tolerant of implementation changes
  that preserve correct behavior. Prefer structural asserts (the retry link
  target, the absence of the action chrome) over literal asserts (the toast
  text, the button label).
- If a test is useless, delete it. A test that passes on nearly any page, or
  that pins today's rendering choice against the documented roadmap, proves
  nothing and constrains the planned feature.

## Where tests live

- `examples/showcase/tests/` — the integration suite: HTTP requests against the
  runnable admin, asserting status codes, redirects, rendered structure, and
  database state.
- `#[cfg(test)] mod tests` at the bottom of a source file — unit tests for pure
  decisions (escaping, state decoding, hook contracts).
- `crates/tablo-ui/assets/*.test.js` — the browser-asset suites, run with
  `node --test`. Each suite's header names the behavior it protects; DOM halves
  are covered by the integration suite instead.
- `xtask/tests/it.rs` — the two contract guards (asset hooks, registry sync);
  edge cases live as unit tests in `xtask/src/lib.rs`.

## Shared harness

`crates/tablo-test` holds the protocol helpers both integration suites share:
`TestClient`, the body readers, the form and multipart writers, the cookie
jar and `Set-Cookie` parsing, and the robust `input_value`. The `auth`
feature (on by default, mirroring `tablo-core`) exposes the cookie and
session helpers the auth suites use. What names crate-local models stays per
crate: the seed and database fixtures, the showcase login and session-mint
flow, the core panel builders and free POST/GET helpers, and the
showcase-only scrapers.

The shared `multipart_body` emits no per-part `Content-Type`: neither server
parser reads one. The framework parser tells file parts from text parts by
the `filename` parameter alone, and the showcase media upload reads the
part's content type only as an image-or-file kind hint defaulting to file. A
suite that asserts on the kind (the media cases) builds that body ad-hoc with
a caller-chosen content type.
