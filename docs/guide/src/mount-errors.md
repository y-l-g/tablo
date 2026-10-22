# Mount errors

`.panel(..)` checks every declaration before the panel serves a request, and returns a
`MountError` listing each mistake it found, not only the first. A mistake the type system cannot
catch, such as a unique field without a unique index or a slug two resources share, therefore
stops the app at startup instead of answering one request wrong.

Each `DeclarationError` names the resource or page type that declares it (`None` for the panel's
own configuration), the `Site` of the declaration it is in, such as the table or the form, and a
`DeclarationErrorKind`, whose `Display` is the message the app prints:

```text
panel '/admin' cannot mount:
  - `showcase::app::PostResource` form: field `slug` is marked unique, but no unique index covers its column: …
```

[`DeclarationErrorKind`](https://y-l.fr/tablo/nightly/api/tablo/enum.DeclarationErrorKind.html)
lists every kind, when the mount refuses it and how to fix it. A kind that groups several faults,
such as `Tenancy` or `ActionInput`, carries a fault enum that says which one, with its own fix.

A test matches on the kind rather than the message; see
[Testing](./testing-and-benchmarks.md#testing-a-panel). The kinds are `#[non_exhaustive]`, so a
`match` on them ends in `_`.

One kind, `NotMounted`, refuses no mount: it answers a request with a 500 when app code asks for a
resource the context's panel does not register, such as through `scoped_query` from a public
page that entered no panel ([Data access](./data-access.md#outside-a-panel)).
