# Upstream notes

`topcoat` and `toasty` are version dependencies on crates.io. A
model's training data may postdate neither release: verify every upstream API from
the versioned sources, never from memory.

## How

- Read the API from the local cargo registry cache or a sibling checkout
  (`../topcoat`, `../toasty` when present), at the version `Cargo.lock` pins — not
  at their branch tip, which has moved on.
- Quote the source file and symbol that proves the signature, the behavior, or
  the absence you rely on.
- When the pinned version changes, re-verify the claims that cited it.

For temporary live-editing against a local checkout, add an uncommitted
`[patch]` section redirecting to `../topcoat` / `../toasty`, per `CONTRIBUTING.md`
"Dependency pins".
