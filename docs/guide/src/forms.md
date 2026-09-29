# Forms

Create and edit forms: the record form a submission parses into, typed lenses, layout blocks, the
fields and what each one guarantees, file uploads, and how validation failures are reported.

## The record form

A resource with a create or edit form declares one struct, `#[derive(RecordForm)]`, with one field
per model column the form writes. A field's name is the model field's name and its type is the
model field's type, so a renamed or retyped column is a compile error:

```rust
#[derive(tablo_core::RecordForm)]
#[record_form(model = User)]
pub struct UserForm {
    pub name: String,
    pub email: String,
    #[record_form(blank = "member")]
    pub role: String,
    #[record_form(blank = true)]
    pub active: bool,
    #[record_form(blank = 0)]
    pub age: i64,
}
```

A field is a **scalar** — `String`, a `TypedValue` type, or an `Option` of one — bound to the key
its control posts, or an **embedded value** marked `#[record_form(embed)]` (an `EmbeddedForm`
type, bound to every key it occupies and written whole). A column the form does not write stays
off the struct: a gated resource's `tenant_id` is stamped by the framework on create, and a column
like `created_at` takes a toasty `#[default(..)]` on the model. The derive also emits
`UserFormField`, one variant per field.

`#[record_form(blank = <expr>)]` is what an empty submission stores. `String` answers `""` and
`Option<T>` answers `None` without one; any other type needs `blank` wherever its control may be
left empty.

The resource names the struct as its `Form` and declares the form's schema in `form()`:

```rust
impl Resource for UserResource {
    type Model = User;
    type Form = UserForm;

    fn table(cx: &Cx) -> Table<User> { /* … */ }

    fn form(_cx: &Cx) -> Schema { /* the controls, as below */ }

    fn validate_record(_cx: &Cx, form: &UserForm) -> FieldErrors<UserForm> {
        let mut errors = FieldErrors::new();
        if form.age < 0 {
            errors.add(UserFormField::Age, "Age must be zero or more");
        }
        errors
    }
}

Panel::new("admin").resource::<UserResource>()
```

A resource with a record form gets the create page, the edit page, and the relationship-options
endpoint. A list-only resource names `type Form = NoForm<Self::Model>;` and leaves `form()` at its
empty default; it gets none of them.

`create_record` and `update_record` default to the derived write, so a resource whose write is
"store what the form says" declares neither. One that checks something inside the transaction
overrides the record fn and delegates:

```rust
async fn update_record(
    cx: &Cx,
    record: Comment,
    posted: Posted<CommentForm>,
    ex: &mut dyn toasty::Executor,
) -> Result<Comment> {
    // `Posted` derefs to the form: an unposted `post_id` reads as the stored one.
    ensure_post_in_tenant(cx, posted.post_id, ex).await?;
    tablo_core::write_update::<Self>(cx, record, posted, ex).await
}
```

A record fn that writes more than the form owns uses the builders: `form.into_create()` is the
model's create builder with every field set, and `posted.into_update(&mut record)` is the instance
update builder with one assignment per field the submission named, or `None` when it named none.

What a submission does:

- **An unposted key keeps its value.** On edit, every declared key the submission does not post is
  filled from the stored record before validation and the parse, and the write assigns only the
  fields the submission named, plus the model's own `#[update(..)]` defaults and `#[version]` bump.
  An emptied control is posted, so it stores the field's blank answer; an emptied leaf of an
  embedded value stores the leaf type's `Default` (ADR-0019), so an emptied `jiff::Timestamp` leaf
  stores the Unix epoch. An API client can post one field of an edit.
- **Errors render in one round.** Schema rules, the unique probe, a value the form's type refuses,
  and `validate_record` render inline with a 200 and write nothing. `validate_record` sees a whole
  form, so it runs once every field parses.
- **`Panel::build` checks the struct against the schema**: a `NoForm` resource declares no
  schema; every control is bound by exactly one
  field and every field's key is a declared control; an optional control, or one inside a
  `Repeater` or a variant group, binds a field with a blank answer; a gated resource's form does
  not claim its tenant column; and, where `can_create` allows it, every non-nullable column is a
  form field, filled by toasty (`#[auto]`, `#[default(..)]`), the stamped tenant column, or named in
  `Resource::CREATE_COLUMNS` by a create override that sets it.

## Controls

Forms use typed lenses, not string paths:

```rust
Schema::new((
    Section::new("Account").schema((
        TextInput::r#for(User::fields().email()).email().unique(),
        Select::r#for(User::fields().role())
            .options(vec!["admin".into(), "member".into()]),
    )),
    Grid::new(2).schema((
        TextInput::r#for(User::fields().name()),
        FileUpload::r#for(MediaAsset::fields().path()),
    )),
))
```

What to know:

- Layout blocks: `Section`, `Group`, `Grid`, `Tabs`. Fields: `TextInput`, `Textarea`, `Select`,
  `FileUpload`, `Repeater`. Every field takes a typed lens (`User::fields().email()`), never a string
  path. `Textarea` is the multi-line half of `TextInput` — same lens, same required default, same
  error contract, a `<textarea>` control instead, and no `unique()` (the app-side probe is built from
  `TextInput`, GH #184).
- `required` defaults to the column nullability. Use `.optional()` to opt out. A bare `Select` over a
  non-nullable FK rejects `""` inline instead of failing at the driver.
- `email()` applies the `email_address` grammar at the form edge: a text domain needs two labels
  (`a@b` and `a@b..c` are refused), a display name is a header rather than an address, and the whole
  address is capped at 254 octets (RFC 5321 §4.5.3.1.3). A quoted local part
  (`"a b"@example.com`), a unicode address (`用户@例え.jp`) and a bracketed domain literal
  (`a@[127.0.0.1]`) pass.
- **A non-`String` column binds through `TextInput::typed`** (GH #192):
  `TextInput::typed::<User, i64>(User::fields().age())` renders the value's
  `Display`, parses the submission through the type's own `FromStr`, and refuses what it cannot parse
  as an inline field error naming the input — `` `twelve` is not a valid whole number `` — instead of
  a 500 or a silent default. What is stored is `Display` of the parsed value, so a value re-submitted
  unchanged is written back in the shape it was read. `typed_context(cx, path)` is the
  embedded/document sibling, as `r#for_context` is to `r#for`. `TypedValue` covers the integer types,
  `f64`, `Uuid` and `jiff::Timestamp`; a type needing its own words implements the trait. A
  `jiff::Timestamp` leaf renders `type="datetime-local"`: the control carries no zone, so the stored
  instant renders in UTC and a submission is read back as UTC. Empty is
  the presence rule's business, not the typed one: a typed column has no spelling for "no value", so
  an empty submission on an optional typed field stores the record form field's blank answer.
- `unique()` does two things. It adds an app-level pre-check — Toasty exposes no unique-violation
  predicate yet, so the DB constraint stays the final guard and concurrent writes can race — and it
  implies **presence**: the framework stores `""` rather than NULL, so an empty value on a unique
  field is refused inline as `"<Label> is required"` instead of being written past an index that
  admits only one (GH #189). `.optional()` does not lift that rule, and `Panel::build` refuses a
  `unique()` marker on a column with no unique index (single-field or composite, `#[unique(a, b)]`
  included), so the declaration and the database cannot disagree about which fields are unique.
- Relation select validates the FK against the related resource query before the write runs:

```rust
Select::r#for(Post::fields().author_id())
    .relationship::<AuthorResource>(
        AuthorResource::query,
        |a: &Author| a.id,
        |a: &Author| a.name.clone(),
    )
    .label("Author")
```

- Relation options are bounded to 200 (`MAX_RELATIONSHIP_OPTIONS`) and memoized per
  `(request, tenant)`. Small tables validate against the bounded set; `can_view` filters before
  labels, `can_view_any`/tenant denial fails closed (`not available`).
- Large reference tables (10k+ rows) need `.searchable()` on the `Select` (GH #150): over-cap
  searchable selects degrade to type-to-search instead of a retry error. Typing fetches
  `GET {parent_list_url}/options?field=&q=` (debounced 200ms, abort in-flight, selection preserved),
  which reuses the related `Table`'s declared `searchable()` columns (`search_expr`), bounds to 200,
  and filters `can_view` before labels. No searchable columns → hard-cap path (non-searchable keeps
  the cap error). Overflowed submits validate via a targeted PK check (the tenant-scoped query +
  `can_view`): legitimate FKs beyond the cap pass, hidden → `invalid`, denied → `not available`, DB
  failure → retry. Initial render keeps the stored value + search input + “Too many options — type
  to search” hint; no-JS keeps the plain select (other fields still submit, relation cannot be
  changed past the cap).

- `FileUpload` binds a `String` path and owns the request half: no `value` on `type=file`,
  `enctype="multipart/form-data"` when a form has one, a 10 MiB body cap (413), a 400 for multipart
  without a boundary, and sanitized basenames (`.` / `..` / Windows reserved names surface as inline
  errors). On an edit the control drops native `required` (GH #184) — a `required` file input cannot
  be pre-filled, so it blocked every untouched save; `required` still holds on create. A value reaches
  the field only from a file part (the uploader's own answer, a re-rendered form's carried upload
  included), from the record on an untouched edit, or as empty on `clear_<field>`; text typed under
  the field's name is dropped (GH #277).
- **Where the bytes go is the app's** (GH #188, ADR-0017): install an `Uploader` once with
  `Panel::uploads(store)`. `store(filename, bytes) -> Result<String, String>` receives the sanitized
  name and the content (bounded by the cap) and returns the value the record stores; a refusal is an
  inline error (`"<Label> could not be uploaded: <reason>"`), not a 500. With no uploader installed
  the sanitized basename is stored — the default — and the bytes are drained rather than buffered.
  A form that re-renders with errors carries the path that store just returned in a hidden
  `keep_<field>` control, because the browser's file input is empty on the next attempt: implement
  `Uploader::holds(path)` to make that carry survive the submit (GH #297). The framework re-uses a
  carried path only when `holds` confirms the store still has it — `holds` must answer `true` only
  for a path the store itself produced and resolves inside its own root — and it defaults to `false`,
  which keeps the behaviour of a store that does not implement it: no carry, so the next submit
  fails `required` (create) or an edit keeps the record's stored file.
- The stored path renders as a link to the file, on the edit form and on the detail page (GH #242),
  only when it is a rooted path or an absolute `http(s)` URL (GH #277): the framework reads no
  extension and renders what the app stored, inventing no URL convention, and any other value — a
  bare basename, a `javascript:` scheme — renders as text rather than a clickable `href`.
  `Panel::serve_dir(path, dir)` mounts the directory an upload store writes to. A served directory is
  **public** (ADR-0017): its URLs answer whoever asks, with no session, because the auth gate covers
  only the panel prefix and `/_topcoat/runtime` (ADR-0013) and a served directory is mounted outside
  both. An app that needs protected files owns that route itself. Every stored value also offers a
  `clear_<field>` checkbox ("Remove the current file"), the one control that means "remove" rather
  than "keep"; an empty file input still means keep. Clearing does not waive `required` — a resource
  whose records may lose their file declares `.optional()`.
- `Repeater` is a single-entry group. An all-empty group is skipped, so its inner required fields do
  not fail the submit. A `required` repeater yields one label-keyed error; a partially filled group
  still enforces inner `required`. An embedded enum's variant groups follow the discriminant a
  submission names (GH #191): only the named variant's fields validate, so a stale value in a group
  the user cannot see never blocks the submit (GH #297). A submission that names no variant hides
  nothing, because the value codec's payload fallback may still read any group.

Validation errors render inline per field. On create an absent key validates as `""`; on edit it
validates as its stored value. Handlers reject unknown form keys with 400 (`role` / `tenant_id`
smuggling fails closed; only `csrf_token`, `clear_<field>` and `keep_<field>` are exempt), and the
record form parses declared keys only, so an extra posted key never reaches a write.

Embedded values declare one form node and flatten their fields; see
[Data access](./data-access.md#embedded-values).
