# Extension points

The seams for reaching outside a resource declaration. Four have full sections
of their own: [`Authenticator`](./policy-auth-tenancy.md),
[`Uploader`](./forms.md), [`Panel::layout_shell`](./panel-and-routing.md), and
[`wired_table`](./data-access.md). The remaining three are documented here.

## Relationship option sources

A relationship choice field loads its options through
`tablo::schema::OptionSource`. Every resource is one, answering from its def
as the request's panel mounted it: `scoped_query` states the tenant-scoped
seed query, `allows` gates rows per ability (default deny), and
`requires_tenant`, `search_expr`, and `order_by` shape scoping, search, and
ordering. Loads are bounded to 200 options, memoized per request, and fail
closed on denial.

## Table state values

`tablo::Sort` (`?sort=<column>&dir=asc|desc`) and `tablo::Cursor` (`?after=`
/ `?before=`) are the parsed sort and pagination of a list URL; see
[Tables](./tables.md). Cursors are opaque: conflicting cursors parse as none
and render the first page.

## Notifications

An `Action` result flashes a `tablo::Notification`, rendered in the panel
shell and stored as a one-time hardened cookie. Build one with
`Notification::success` / `error` / `info` / `warning` plus `.description()`,
and store it with `tablo::notification::set_notification`; see
[Security](./security.md).
