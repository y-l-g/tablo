# Extension points

The seams for reaching outside a resource declaration. Four have full sections
of their own: [`Authenticator`](./policy-auth-tenancy.md),
[`Uploader`](./forms.md), [`Panel::layout_shell`](./panel-and-routing.md), and
[`wired_table`](./data-access.md). The remaining three are documented here.

The traits that add a new kind of building block live in `tablo::extend`: a
[`Column`](./tables.md#your-own-columns), a [`Filter`](./tables.md#filters), a
[`Control`](./forms.md#custom-controls), an `OptionSource` and a
[`TypedValue`](./forms.md#typed-values). The built-in columns and filters and
the `Toggle` control implement the same traits.

## Relationship option sources

A relationship choice field loads its options through
[`tablo::extend::OptionSource`](https://y-l.fr/tablo/nightly/api/tablo/extend/trait.OptionSource.html). Every resource is one,
answering from its def as the request's panel mounted it, so a choice over a resource offers what
its list would show the same user. Implement it for a source that is not a resource, such as a
lookup table no panel lists; the trait's page says what each method states and how a load is
bounded.

## Table state values

`tablo::Sort` (`?sort=<column>&dir=asc|desc`), `TableState::page` and
`per_page` (`?page=`, `?per_page=`), and `tablo::Cursor` (`?after=` / `?before=`)
are the parsed sort and pagination of a list URL; see [Tables](./tables.md).
`tablo::TablePage::position` says where a numbered page sits.

## Notifications

An [action](./actions.md)'s result flashes a `tablo::Notification`, rendered in the panel
shell and stored as a one-time hardened cookie. Build one with
`Notification::success` / `error` / `info` / `warning` plus `.description()`,
and store it with `tablo::notification::set_notification`; see
[Security](./security.md).
