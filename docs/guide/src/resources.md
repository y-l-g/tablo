# Resources

The `Resource` trait: how one Toasty model maps to its admin UI, what the framework checks at boot,
and how tenancy is declared.

One resource maps one Toasty model to its admin UI:

```rust
pub trait Resource: Sized + Send + Sync + 'static {
    type Model: toasty::schema::Model + Send + Sync + 'static;
    type Form: RecordForm<Model = Self::Model>;     // required: a record form, or NoForm
    fn query(_cx: &Cx) -> Query<List<Self::Model>>; // default: Query::all()
    fn view_query(_cx: &Cx) -> Query<List<Self::Model>>; // default: query(cx)
    fn view_values(_cx: &Cx, _record: &Self::Model)
        -> HashMap<String, String>;                 // default: empty
    fn table(_cx: &Cx) -> Table<Self::Model>;       // required: Table::new(key, columns)
    // plus can_* policy fns (default deny), slug/navigation/requires_tenant
    // defaults, form()/validate_record and the record fns
}
```

Every resource registers with `Panel::resource`, which serves the list, the detail page, delete,
bulk delete, and export. A resource whose `Form` is a `#[derive(RecordForm)]` struct also gets the
create page, the edit page, and the relationship options
([Forms](./forms.md#the-record-form)); a list-only resource names
`type Form = NoForm<Self::Model>;` and links to no create page.

## The contract

Every item except `Model`, `Form`, and `table()` is defaulted, so a resource names its model and
its form and declares its list view, and any other omission has to fail loudly instead of quietly:

- **At `Panel::build`** (which returns `Result<Router>`): a
  `NoForm` resource whose `can_create` is on, or whose `form()` declares a schema,
  fails the build, since it serves no form; a record form must agree with its `form()` schema
  ([Forms](./forms.md#the-record-form)). `table()`, `form()` and
  `can_create()` are declarations: `Panel::build` calls them with a Db-only context to check them,
  and each list and form request calls `table()` / `form()` again, so a declaration must not need
  request-scoped context.
- **Chrome follows the declarations, gated per record**: the row Delete control, the bulk column,
  and the bulk bar render when `can_delete_any()` allows, the Edit link when the resource has a
  record form, and the View link when it declares `view()`. `can_delete_any()` defaults to `false`
  and the delete handlers check it too, so a resource that never mentions it renders no delete
  affordance and answers every delete POST with 403. `can_delete()` defaults to `can_delete_any()`;
  override it to refuse some rows. The panel applies the row predicates **per row** through the
  table's row policy (GH #235), so a row `can_update()` refuses renders no Edit link, a row
  `can_delete()` refuses renders no Delete link and no bulk checkbox, and a row `can_view()`
  refuses renders no View link. Select-all therefore submits only the rows the handler will accept
  — the showcase's SSO-guarded user is the worked example: its row keeps the View link and nothing
  else. The handler keeps its all-or-nothing check on the POST as the safety net for a hand-crafted
  request.
- **Default-deny stands**: every `can_*` defaults to `false`, except `can_delete()`, which defaults
  to `can_delete_any()`; an unconfigured resource exposes no data and no mutation.

## What to know

- `slug()`, `label()`, and `navigation_label()` have working defaults. `label()` is one record's
  name, used in the "Create" and "Edit" titles; `navigation_label()` pluralizes it for the sidebar
  and the list title. Override `label()` to rename, and `navigation_label()` only for a plural the
  rules cannot guess.
- `navigation()` curates this resource's sidebar entry: override it to change the label, the `order`
  (lower renders first, ties keep declaration order) or the URL, e.g.
  `NavigationItem { order: -1, ..NavigationItem::for_resource::<Self>() }`. The URL is the Panel's
  call: `for_resource` names none, so the panel that mounts the resource resolves it to
  `{prefix}/{slug}`, and a resource never links at `/admin` on a panel mounted elsewhere. Spell a
  URL out instead (`NavigationItem::at(..)`) only to link somewhere other than the resource's list
  page — the Panel keeps it verbatim.
- `query()` is the seam for the resource's **own** row scoping: soft deletes and row-level
  visibility. Every loader starts from it, as does app code that calls `scoped_query`. Tenancy is not
  its job: when `requires_tenant()` is `true` the framework derives the `tenant_id` filter from the
  model's own schema and ANDs it onto whatever `query` returns, at every loader (GH #223), so
  restating it here is redundant. Relations are not its job either (ADR-0018): the list and the
  export load the relations their columns declare with `TextColumn::include(..)`, and the detail
  page loads `view_query`. Include a relation in `query` only when every loader needs it, such as one
  `can_view` reads. The option loaders run `query`, so an option label projects the related record's
  own columns.
- `view_query()` is the detail page's query: `query` plus the relations `view_relations` reads,
  e.g. `Self::query(cx).include(comments)`. The framework ANDs the tenant scope onto it too.
- `table()` and `form()` are hand-written, and so is the impl itself: a resource is `type Model` plus
  whichever hooks it uses. There is no `Resource` derive (GH #222); the macros crate ships
  `derive(EmbeddedForm)` (GH #191) and `derive(RecordForm)` (GH #369), which types the form's
  values rather than declaring the resource.

```rust
struct UserResource;

impl Resource for UserResource {
    type Model = User;
    type Form = NoForm<Self::Model>;
}

struct PostResource;

impl Resource for PostResource {
    type Model = Post;
    type Form = NoForm<Self::Model>;

    // the resource's own scoping seam, spelled out where it is used
    fn query(cx: &Cx) -> Query<List<Post>> {
        // soft deletes, row-level visibility, includes — not the tenant filter
    }
}
```

- Record fns (`create_record` / `update_record`, `delete_record`,
  `bulk_delete_records`) do the writes. Handlers load records, check policy, then call them in a
  transaction. `create_record` and `update_record` return the row they wrote — the create builder
  hands the created one back and a Toasty instance update reloads the model, so both are already in
  hand — because that is the only way the framework can name what a write committed (GH #112). The
  derived `write_update` ends with the reloaded record, and a model used by a `Resource` derives
  `Clone`. `delete_record` defaults to deleting the row by its record key through `scoped_query`,
  and `bulk_delete_records` loops over `delete_record`, so one override (a soft delete, say) covers
  both.
- `after_commit(cx, committed)` is the post-commit seam (GH #112): called once per committed write,
  after the transaction and before the response, with a `Committed` naming the mutation
  (`Mutation::Create/Update/Delete`) and the rows it wrote (a bulk delete is one call with all of
  them). It is where email, webhooks, audit rows and cache invalidation belong — running them in a
  record fn leaks the effect on a rollback, and the transaction's pool discipline forbids a second
  handle while it is open. The default is a no-op, a hook failure is logged without touching the
  committed write, and it never runs when nothing committed.
- The edit form hydrates from `RecordForm::hydrate`, which the derive generates.
  `view_values(cx, record)` is the detail page's projection for a list-only resource, and adds any
  key only the view shows for a form resource. `cx` is the request's: a scalar projection needs
  nothing from it, but an embedded value's keys come from the compiled mapping (GH #191).

## Tenancy

Tenancy is declared, not restated per query. The pattern:

```rust
impl Resource for PostResource {
    type Model = Post;
    type Form = NoForm<Self::Model>;

    // `Post` declares `tenant_id: uuid::Uuid`. Declaring this is the whole
    // tenant contract: the gate is GH #87 — every handler 403s without a
    // tenant — and the scope is GH #223 — every loader (list, edit, delete,
    // bulk, export, relationship options) ANDs `tenant_id = <request tenant>`,
    // derived from the model's own schema, onto whatever `query` returns.
    fn requires_tenant() -> bool {
        true
    }

    // The resource's *own* scoping only — a soft delete, row-level visibility,
    // the includes a page loads. Writing the tenant filter here is redundant:
    // the framework derives it at every loader, so a copy that disagreed with
    // the derived column could only hide rows, never widen access.
    fn query(_cx: &Cx) -> Query<List<Post>> {
        Query::<List<Post>>::all()
    }
}
```

App code that loads rows outside the framework's loaders — a record fn double-checking a foreign
key, a custom page — calls `scoped_query::<PostResource>(cx)?` rather than `PostResource::query(cx)`:
on a gated resource that method is the **tenant-unscoped** base, deliberately, so the
tenant-unscoped case is visible at the call site (it still carries whatever the resource's own
`query` scopes — soft deletes included).

Three shapes, one gate:

1. **The model carries the tenant** (the common case): `requires_tenant() =
   true` and nothing else. The framework derives `tenant_id = <request tenant>`
   from the model's own schema.
2. **The row inherits its tenant** — `Comment` has no `tenant_id`, it belongs to
   a post that does. Declare the predicate instead of the column:

   ```rust
   fn requires_tenant() -> bool {
       true
   }

   fn tenant_scope(tenant: uuid::Uuid) -> Option<toasty::stmt::Expr<bool>> {
       // The framework ANDs this onto `query`/`view_query` at every loader,
       // exactly as it ANDs the derived filter elsewhere.
       Some(Comment::fields().post().tenant_id().eq(tenant))
   }
   ```

   The gate is what matters: writing this filter inside `query` with
   `requires_tenant() = false` would *skip* it for a tenantless request rather
   than refuse it, serving every tenant's rows to a tenantless admin.
3. **The resource must serve more than one tenant** (a deliberate cross-tenant
   view): `requires_tenant() = false` and scope in `query` by hand — the one
   explicit way out, and the gate goes with it.

A gated resource that supplies no predicate at all — no `tenant_id` column and
no `tenant_scope` override — fails `Panel::build` (GH #231), the same boot
failure any other misdeclaration gets, and every loader keeps answering an error
naming itself rather than querying tenant-unscoped — the backstop for a
predicate that is only `None` for some tenants. There is no override that
removes the scope.

The request's tenant comes from the logged-in user; see
[Policy, auth, tenancy](./policy-auth-tenancy.md).
