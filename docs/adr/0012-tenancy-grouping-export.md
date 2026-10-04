# Tenancy via Cx, in-memory grouping and CSV export

Date: 2026-08-31 — Status: accepted

## Decision

**Tenancy.** `tenant_id(cx)` answers the request tenant: a `Tenant(uuid::Uuid)` request extension
or `Cx`-scoped value when server code sets one, else the signed-in user's selected membership
(ADR-0013). `Resource::tenancy` defaults to `Tenancy::none()`; the framework applies the tenant
predicate to every loader through `scoped_query` (ADR-0002). Tests inject `Tenant` through
request extensions.

**Grouping.** `Table::group_by(lens)` groups by a field, named after it; `TableState`
parses `?group_by=`; render groups page rows in memory through a `BTreeMap` with `{key} ({count}
on this page)` headers. A page-local sum waits for upstream `GROUP BY`; `Table::group_by` then
delegates without changing resources.

**Export.** `Table::csv_header` and `Table::csv_row` generate RFC4180 fragments. `Panel` owns `GET
{prefix}/{slug}/export` serving `text/csv` with a sanitized attachment filename. The loader walks
the filtered, sorted query in cursor chunks; the cap counts viewable rows after visibility. A
full scan window with rows left refuses with 413, so the export never returns a partial file. It
loads the tenant-scoped query with the relations exported columns include (`ComputedColumn::include`,
ADR-0018).
