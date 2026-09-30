# Resources

A resource is the admin for one Toasty model: which rows it lists, how its table and form look,
who may see and change a record, and how a write runs. You implement the `Resource` trait on a
unit struct and register it with `Panel::resource`.

The smallest resource lists rows and nothing else:

```rust
pub struct AuditResource;

impl Resource for AuditResource {
    type Model = Audit;
    type Form = NoForm<Audit>; // list-only: no create or edit pages

    fn can_view_any(_cx: &Cx) -> bool {
        true
    }

    fn table(_cx: &Cx) -> Table<Audit> {
        Table::new(
            |a: &Audit| a.id.to_string(),
            TextColumn::r#for(Audit::fields().action(), |a: &Audit| a.action.clone()),
        )
    }
}
```

A resource with create and edit pages names a `#[derive(RecordForm)]` struct as its `Form` and
declares the form's controls in `form()`; see [Forms](./forms.md).

## Trait items

Only `Model`, `Form` and `table()` are required. Every other item has a default.

| Item | Default | Purpose |
| --- | --- | --- |
| `type Model` | required | the Toasty model; must be `Clone + Send + Sync` |
| `type Form` | required | a record form, or `NoForm<Self::Model>` for a list-only resource |
| `table(cx)` | required | the list's columns, filters and options: [Tables](./tables.md) |
| `form(cx)` | no controls | the create and edit form's controls: [Forms](./forms.md) |
| `validate_record(cx, form)` | no errors | rules that need the whole parsed form |
| `view(cx)` | nothing | the detail page's fields; the page exists only when this declares some: [Detail pages](./detail-pages.md) |
| `view_values(cx, record)`, `view_content(cx, record)` | none | what the detail page shows beyond the form's fields |
| `view_query(cx)` | `query(cx)` | the detail page's query, with the relations it reads |
| `record_label(cx, record)` | `None` | the detail page's heading |
| `public_url(cx, record)` | `None` | a link to the record's public page on its detail and edit pages |
| `relations()` | none | related resources shown as tables on the detail and edit pages |
| `query(cx)` | every row | the base query every loader starts from: [Scoping](#scoping-the-query) |
| `requires_tenant()`, `tenant_scope(tenant)` | not tenant-owned | tenancy: [Policy, auth, tenancy](./policy-auth-tenancy.md#tenancy) |
| `can_view_any`, `can_view`, `can_create`, `can_update`, `can_delete_any` | `false` | policy: [Policy, auth, tenancy](./policy-auth-tenancy.md#policy) |
| `can_delete(cx, record)` | `can_delete_any(cx)` | per-record delete policy |
| `create_record`, `update_record` | the derived write | the create and update writes: [Writes](#writes) |
| `delete_record`, `bulk_delete_records` | delete by primary key | the delete writes |
| `after_commit(cx, committed)` | nothing | side effects after a write commits |
| `CREATE_COLUMNS` | none | columns an overridden `create_record` sets itself |
| `slug()`, `label()`, `navigation_label()` | from the type names | URLs and titles: [Naming](#naming) |
| `navigation()` | the default entry | the sidebar entry: [Sidebar](./panel-and-routing.md#sidebar) |

## Naming

The names default from the type names, following Filament's conventions:

| Item | Default | `BlogPostResource` over `BlogPost` |
| --- | --- | --- |
| `slug()` | resource name without `Resource`, pluralized, kebab-cased | `blog-posts` |
| `label()` | the model's type name; used in "Create …" and "Edit …" | `BlogPost` |
| `navigation_label()` | `label()` pluralized; the sidebar entry and list title | `BlogPosts` |

Override `label()` to rename a record, and `navigation_label()` only when the plural rules guess
wrong. Name resources in the singular: `UsersResource` pluralizes to `userses`.

## Scoping the query

`query(cx)` is the base query of every loader: the list, the export, the edit and delete
handlers, relationship options and the detail page. Use it for the resource's own row scoping,
such as hiding soft-deleted rows:

```rust
fn query(_cx: &Cx) -> Query<List<Post>> {
    Query::<List<Post>>::all().filter(Post::fields().deleted_at().is_none())
}
```

Two things do not belong in `query`:

- **The tenant filter.** For a tenant-owned resource the framework adds it to `query` at every
  loader. See [Tenancy](./policy-auth-tenancy.md#tenancy).
- **Relations.** The list and the export load the relations their columns declare with
  `TextColumn::include`, and the detail page loads `view_query`. Include a relation in `query`
  only when every loader reads it, for example because `can_view` does.

In your own code, load a resource's rows with `scoped_query::<R>(cx)?`, not `R::query(cx)`:
`scoped_query` is `query` with the tenant filter applied, and returns an error rather than an
unscoped query when the request has no tenant.

The unique-value check on forms probes through the same scoped query, so a `#[unique]` index
wider than the scope is invisible to it: the check misses the collision and the database refuses
the write with a 500. Scope such indexes to match, as in `#[unique(tenant_id, email)]`.

## Writes

Every create, update and delete runs in a transaction the framework opens. For an update or a
delete, the handler first loads the target record through the scoped query inside that transaction
and checks policy on it. It then calls the resource's record function with the open transaction as
`ex`.

`create_record` and `update_record` default to writing the record form's fields
(`write_create` and `write_update`), so most resources declare neither. To check something inside
the transaction, override the function and delegate:

```rust
async fn update_record(
    cx: &Cx,
    record: Comment,
    posted: Posted<CommentForm>,
    ex: &mut dyn toasty::Executor,
) -> Result<Comment> {
    // `Posted` derefs to the form.
    ensure_post_in_tenant(cx, posted.post_id, ex).await?;
    tablo_core::write_update::<Self>(cx, record, posted, ex).await
}
```

Run every statement through `ex`, and use the `record` you are given rather than loading it
again: it is the row the policy check passed. Both functions return the written row. An error
rolls the transaction back and nothing is written.

`delete_record` deletes the row by its primary key, and `bulk_delete_records` calls
`delete_record` once per record in one transaction, so overriding `delete_record` — for a soft
delete, say — covers both. A bulk delete is all-or-nothing.

### After the commit

`after_commit` runs once per committed write, after the transaction and before the response. Put
side effects there — email, webhooks, audit rows, cache invalidation — so a rolled-back write never
triggers them:

```rust
async fn after_commit(cx: &Cx, committed: Committed<Post>) -> Result<()> {
    for post in committed.records() {
        notify_subscribers(cx, post).await?;
    }
    Ok(())
}
```

`Committed` names the mutation (`Mutation::Create`, `Update` or `Delete`) and the rows written: the
created or updated row, or every deleted row in one call for a bulk delete. The hook is not called
when nothing committed. An error it returns is logged; the write stays committed.

## Startup checks

`Panel::build` calls each resource's declarations once, with a context that holds only the
database, and refuses the resource when:

- `table()` or `view()` is malformed: a duplicate column or field name, a zero page size, a
  modifier on the wrong kind of field;
- the record form and `form()` disagree: a control no form field binds, a form field with no
  control, an optional control whose field has no blank value, a `unique()` field with no
  unique index, or a tenant-owned resource's form claiming its tenant column;
- `can_create` is allowed and a non-nullable column is set by nothing: not the form, not a Toasty
  default, not the tenant stamp, and not listed in `CREATE_COLUMNS`;
- a `NoForm` resource declares `form()` or allows `can_create`;
- `requires_tenant()` is `true` and no tenant predicate can be derived;
- a relation names a resource the panel does not register, or names one twice.

Because of this call, `table()`, `form()`, `view()` and `can_create()` must not depend on the
request: a check that reads the current user or tenant sees an anonymous request at startup.
