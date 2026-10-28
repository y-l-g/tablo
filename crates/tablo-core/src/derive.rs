//! The derive macros: what each writes, and the attributes it reads.

/// Derives `EmbeddedForm` for an embedded struct or enum.
///
/// Builds the schema node and converts the value through that node's keys.
///
/// ```rust,no_run
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     publication: Publication,
/// # }
/// # use tablo_core::Section;
/// #[derive(Debug, Clone, toasty::Embed, tablo_core::EmbeddedForm)]
/// pub enum Publication {
///     #[column(variant = 1)]
///     Scheduled {
///         #[shared(timestamp)]
///         #[form(label = "Publication timestamp")]
///         scheduled_at: String,
///         scheduled_for: String,
///     },
///     #[column(variant = 2)]
///     Published {
///         #[shared(timestamp)]
///         published_at: String,
///         canonical_url: String,
///     },
/// }
///
/// // form declaration — no field bindings written by hand
/// Section::new("Publication").schema(Publication::form(Post::fields().publication()));
/// ```
///
/// # How a field is classified
///
/// A field marked `#[form(embed)]` is another **embedded value**, delegated to
/// its own `EmbeddedForm`. Every other field is a **scalar**: one column, read
/// and written through `FormScalar` (`String`, a `TypedValue` type, an
/// `Options` enum, or an `Option` of one). A scalar of another type fails to compile at the
/// field, naming the trait. An empty scalar is its blank answer — its declared
/// `#[form(blank = ..)]`, `None` for an `Option`, `false` for a `bool`, `""`
/// for an `#[form(optional)]` `String` — and a scalar with none is required:
/// its control renders required and the parse refuses its key.
///
/// # Which variant an enum reads
///
/// A named discriminant always wins, and an undeclared one is refused;
/// otherwise the first variant, in declaration order, with a **payload of its
/// own** submitted — a `#[shared(..)]` column belongs to several variants and
/// never selects one; otherwise the first variant.
///
/// A `#[shared(..)]` column renders once. A variant that does not declare it hides the column:
/// the codec neither reads nor submits it there, so that variant needs no answer for the
/// column, even when the column is required.
///
/// An enum renders a choice of variant and one group of fields per variant. The page shows
/// only the chosen variant's group, and the variant can change on edit. The submitted variant
/// decides which fields are read and validated, so a stale value in a hidden group never
/// blocks a submit; a hidden group's controls are disabled, so the browser neither validates
/// nor submits them. Without JavaScript only the stored variant's group shows, so a create
/// page offers no variant's fields, and the variant choice still decides.
///
/// # Not supported
///
/// A `#[document]` field, a relation, an enum nested inside an enum variant, and a tuple
/// struct.
///
/// # Per-field attributes
///
/// - `#[form(embed)]` — a nested `EmbeddedForm` value.
/// - `#[form(label = "Canonical URL")]` — the control's label (default: the field name in
///   sentence case, `canonical_url` → "Canonical url").
/// - `#[form(multiline = 3)]` — a `<textarea>` of 3 rows.
/// - `#[form(blank = ..)]` — what an empty submission reads as.
/// - `#[form(optional)]` on a `String` — an empty submission reads as `""`.
///
/// Anything else in `#[form(..)]` is a compile error, as are `label`, `multiline`, `blank` and
/// `optional` on an embedded value, `blank` or `optional` on an `Option`, and `optional` on a
/// type other than `String`.
pub use tablo_macros::EmbeddedForm;
/// Derives [`FormInput`](trait@crate::FormInput) for a typed value a form posts under its own
/// keys: what an [`Action`](crate::Action) asks for before it runs, what a sign-up reads, or
/// one item of a repeater.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, status: String }
/// # use tablo_core::{Action, FormInput, NoForm, Resource};
/// # use topcoat::{Result, context::Cx};
/// # struct PostResource;
/// # impl Resource for PostResource {
/// #     type Model = Post;
/// #     type Form = NoForm<Post>;
/// # }
/// #[derive(tablo_core::FormInput)]
/// pub struct Rejection {
///     #[form(multiline = 4)]
///     pub reason: String,
///     pub notify_author: bool,
/// }
///
/// struct Reject;
///
/// impl Action<PostResource> for Reject {
///     type Input = Rejection;
///     const NAME: &'static str = "reject";
///
///     async fn run(
///         _cx: &Cx,
///         posts: &[Post],
///         rejection: Rejection,
///         ex: &mut dyn toasty::Executor,
///     ) -> Result<tablo_core::Outcome, tablo_core::WriteError> {
///         // Store `rejection.reason`, notify when `rejection.notify_author` holds…
///         # let _ = (posts, rejection.reason, rejection.notify_author, ex);
///         Ok(tablo_core::Outcome::done())
///     }
/// }
/// ```
///
/// Each field posts its own name, which no column binds, and renders the control its type
/// picks: a `bool` a checkbox, `#[form(options)]` a choice over the field type's
/// [`Options`](derive@crate::Options), `#[form(options = T)]` one over `T`'s, and any other
/// [`FormScalar`](crate::FormScalar) a text input of the type's input type.
///
/// Beside the struct, the derive declares its [`Field`](crate::FormInput::Field) enum with the
/// struct's visibility: `RejectionField`, one variant per field in Pascal case
/// (`RejectionField::NotifyAuthor`), which
/// [`Action::validate_input`](crate::Action::validate_input) refuses a value under.
///
/// One item of a repeater is an embedded struct a `#[document]` list stores, which a
/// [`RecordForm`](derive@crate::RecordForm) edits with `#[form(repeat)]`:
///
/// ```rust
/// #[derive(Debug, Clone, toasty::Embed, tablo_core::FormInput)]
/// pub struct Link {
///     pub label: String,
///     #[form(label = "URL")]
///     pub url: String,
/// }
///
/// #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// pub struct Post {
///     #[key]
///     #[auto]
///     id: uuid::Uuid,
///     #[document]
///     #[default(Vec::<Link>::new())]
///     links: Vec<Link>,
/// }
///
/// #[derive(tablo_core::RecordForm)]
/// #[form(model = Post)]
/// pub struct PostForm {
///     #[form(repeat)]
///     pub links: Vec<Link>,
/// }
/// ```
///
/// Each row posts the item's keys under its own prefix (`links.0.url`) and answers to its
/// controls' own rules, such as a choice offering only its options. A repeater takes at most
/// [`MAX_REPEATER_ROWS`](crate::MAX_REPEATER_ROWS) rows.
///
/// A field's **blank answer** is its `#[form(blank = ..)]`, `""` for an `#[form(optional)]`
/// `String`, `None` for an `Option`, or `false` for a `bool`. A field with none is required:
/// its control renders required and the parse refuses an empty submission.
///
/// # Per-field attributes
///
/// - `#[form(label = "Reason")]` — the control's label (default: the field name in sentence
///   case).
/// - `#[form(multiline = 4)]` — a `<textarea>` of 4 rows.
/// - `#[form(placeholder = "rust, async")]` — a text input's placeholder.
/// - `#[form(email)]` — a text input that refuses a malformed email address.
/// - `#[form(url)]` — a text input that refuses what is not an `http` or `https` URL:
///   [`TextField::url`](crate::TextField::url).
/// - `#[form(tel)]` — a telephone text input, in no checked format:
///   [`TextField::tel`](crate::TextField::tel).
/// - `#[form(password)]` — a masked text input on a `String`, read untrimmed, that a refused
///   form renders empty. A repeater refuses one in its item at mount: an edit could not render
///   the stored secret back, so each save would overwrite it.
/// - `#[form(blank = ..)]`, `#[form(optional)]` — the blank answer.
/// - `#[form(options)]`, `#[form(options = T)]` — a choice. An `Option<T>` field names its
///   options type: `#[form(options = T)]`.
///
/// A generic struct, a tuple struct, an empty struct (name `()` instead), an unknown key, a
/// text input's key with `options` or on a `bool`, `password` with `multiline`, two of
/// `email`, `url`, `tel` and `password`, and a field
/// that is not a `FormScalar` are compile errors.
/// [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel) refuses an action's
/// input with a field named `csrf_token`, `confirm` or `ids`, and a sign-up's with one named
/// `csrf_token` or `next`: the keys each POST carries besides its input.
///
/// A hand-written impl names its own [`Field`](crate::FormInput::Field) type, or
/// [`Infallible`](std::convert::Infallible) when no hook refuses it by field, and builds its
/// fields with [`Field::text_input`](crate::Field::text_input),
/// [`Field::choice_input`](crate::Field::choice_input)
/// and [`Field::toggle_input`](crate::Field::toggle_input); its controls render without the
/// `required` mark, though its `parse` still refuses what it requires. Its `write` fills a
/// repeater row from a stored item, and writes nothing for a type no repeater holds.
pub use tablo_macros::FormInput;
/// Derive a model's [`ModelColumns`](trait@crate::ModelColumns): the table a resource lists it
/// in and the detail page it shows it on, when the resource declares neither.
///
/// ```rust
/// # #[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo_core::Options)]
/// # pub enum Role { Admin, Member }
/// #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// pub struct User {
///     #[key]
///     #[auto]
///     pub id: uuid::Uuid,
///     pub name: String,
///     pub role: Role,
///     pub active: bool,
///     #[columns(skip)]
///     pub password_hash: String,
///     #[default(jiff::Timestamp::now())]
///     pub created_at: jiff::Timestamp,
/// }
///
/// // Lists `name`, `role`, `active` and `created_at`, and shows them on the detail page.
/// let table = <User as tablo_core::ModelColumns>::table();
/// # let _ = table;
/// ```
///
/// Each field shows whoever writes it, the person through a form or Toasty through a
/// `#[default]` or an `#[update]`, in a column its type picks:
///
/// - a `bool`: a sortable [`BooleanColumn`](crate::BooleanColumn), yes or no;
/// - a `String` or `Option<String>`: a sortable, searchable [`TextColumn`](crate::TextColumn);
/// - any other [`FormScalar`](crate::FormScalar): a sortable
///   [`TextColumn`](crate::TextColumn), an [`Options`](derive@crate::Options) enum by its
///   label;
/// - an [`EmbeddedForm`](derive@crate::EmbeddedForm) value: an
///   [`EmbeddedColumn`](crate::EmbeddedColumn), leaf by leaf, on the detail page only;
/// - a list of [`FormInput`](derive@crate::FormInput) items: a
///   [`RepeaterColumn`](crate::RepeaterColumn), on the detail page only;
/// - any other type: no column.
///
/// No column shows a key Toasty generates, a `#[version]` field, a relation or its foreign
/// key, which only a declared [`RelationColumn`](crate::RelationColumn) shows, by the related
/// record's title, or a `TenantId` or `Option<TenantId>` field, which belongs to the
/// resource's [`Tenancy`](crate::Tenancy). An `#[auto]` `created_at` or `updated_at` shows
/// like any timestamp. A `#[version]` field is what
/// [`ModelColumns::version`](crate::ModelColumns::version) returns, so the edit form refuses a
/// save another save overtook.
///
/// Every other field shows, a secret such as a password hash or an API token included, to
/// anyone the policy lets view the list: mark it `#[columns(skip)]`, which leaves it out of
/// the table, its search and export, and the detail page. On a field already left out it is a
/// compile error, as are an unknown key and a generic model.
///
/// The derive reads the model, not a form: a file path shows as text and a `String` an
/// `#[form(options = ..)]` key labels shows its stored value. Declare a
/// [`FileColumn`](crate::FileColumn) or a formatted [`TextColumn`](crate::TextColumn) for
/// them.
///
/// A resource replaces either default with [`ResourceDef::table`](crate::ResourceDef::table)
/// or [`ResourceDef::detail`](crate::ResourceDef::detail), and extends one by starting from
/// it: `User::table().filters(..)`, with [`ModelColumns`](trait@crate::ModelColumns) in scope.
pub use tablo_macros::ModelColumns;
/// Derive a model's [`RecordForm`](derive@crate::RecordForm) struct from the model: the fields
/// a person fills, named and typed as the model declares them.
///
/// ```rust
/// #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns, tablo_core::ModelForm)]
/// pub struct Book {
///     #[key]
///     #[auto]
///     pub id: uuid::Uuid,
///     pub title: String,
///     #[form(optional)]
///     pub subtitle: String,
///     #[default(jiff::Timestamp::now())]
///     pub created_at: jiff::Timestamp,
/// }
///
/// // The derive wrote `BookForm { title, subtitle }`, with `BookFormField` and
/// // `BookFormControls`, as `#[derive(RecordForm)]` would.
/// let form = BookForm {
///     title: "Dune".into(),
///     subtitle: String::new(),
/// };
/// # let _ = form;
/// ```
///
/// The form leaves out what no person fills: a key, so an edit never rewrites it, an
/// `#[auto]`, `#[update]` or `#[version]` field, a relation, and a `TenantId` or
/// `Option<TenantId>` field, which belongs to the resource's [`Tenancy`](crate::Tenancy). A
/// form key on one of those is a compile error naming the fix. A `#[default]` field and a
/// `#[belongs_to]` foreign key are left out unless a form key opts them in, such as
/// `#[form(repeat)]` on a defaulted list or `#[form(relationship = AuthorResource)]` on a
/// foreign key. `#[form(skip)]` leaves out any other field. When the policy allows `Create`,
/// mounting refuses a non-nullable column left out, such as a key a person picks, unless an
/// overridden `create_record` names it with `create_column`.
///
/// Each field keeps the model field's visibility and docs. A field's `#[form(..)]` keys are
/// [`RecordForm`](derive@crate::RecordForm)'s, as are the form's checks and defaults.
/// `#[form(name = BookEditor)]` on the model names the form, `BookForm` otherwise. A form
/// whose fields differ from the model's, such as a many-to-many field's keys, is a
/// `#[derive(RecordForm)]` struct.
pub use tablo_macros::ModelForm;
/// Derive `Options` for a unit-variant enum: the `(value, label)` list a
/// choice field, a select filter and a column share, and the `FormScalar`
/// that posts a variant as its value and reads it as its label.
///
/// ```rust
/// # use tablo_core::Options;
/// #[derive(Debug, Clone, Copy, PartialEq, Eq, toasty::Embed, tablo_core::Options)]
/// pub enum Status {
///     Draft,
///     #[option(label = "Live")]
///     Published,
/// }
///
/// assert_eq!(Status::Published.value(), "published");
/// assert_eq!(Status::Published.label(), tablo_core::Text::from("Live"));
/// assert_eq!(Status::from_value("draft"), Some(Status::Draft));
/// ```
///
/// Each variant's value is its `snake_case` name, and its label the
/// [`Text`](crate::Text) message `option-{enum}.{variant}`, the enum in kebab
/// case and the variant in snake case, which reads the variant in sentence
/// case when no catalog holds it: `Status::Draft` reads `option-status.draft`,
/// else `Draft`. `#[option(value = "..")]` and `#[option(label = "..")]`
/// override either, the label as plain text. A generic enum, a variant with fields, two
/// variants sharing one value or one label, and an unknown key are compile errors.
///
/// The enum, and an `Option` of it, is a form scalar through this derive, so
/// it cannot also implement `TypedValue` or `FormScalar` itself.
pub use tablo_macros::Options;
/// Derive `RecordForm` for the typed value a resource's form writes.
///
/// One field per model column the form writes, named and typed like the
/// model's field. A scalar (`String`, a `TypedValue` type, an `Options` enum,
/// or an `Option` of one) binds the key its control posts; a `#[form(embed)]` field binds
/// every key of an `EmbeddedForm` value and is written whole.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # pub struct User {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     name: String,
/// #     role: String,
/// #     age: i64,
/// # }
/// #[derive(tablo_core::RecordForm)]
/// #[form(model = User)]
/// pub struct UserForm {
///     pub name: String,
///     #[form(blank = "member")]
///     pub role: String,
///     #[form(blank = 0)]
///     pub age: i64,
/// }
/// ```
///
/// The derive also emits `UserFormField`, one variant per field, which
/// `Posted` and `Resource::validate_record`'s `FieldErrors` key on and
/// `RecordForm::fields` answers with each variant's keys.
/// It emits `UserFormControls`, one control per field chosen from the field —
/// a `bool` is a toggle, `#[form(options = T)]` a choice over `T`'s options,
/// `#[form(options)]` a choice over the field type's options,
/// `#[form(relationship = R)]` a choice over `R`'s records (a multiple choice over a `Vec` of
/// their keys), `#[form(file)]` a file field, `#[form(embed)]` the embedded value's schema,
/// `#[form(repeat)]` a repeater, and any
/// other field a text field — with `controls()` handing them over, typed by the form so only a
/// `Schema<UserForm>` places them, and `RecordForm::control` answering one
/// field's. A resource's `ResourceDef` defaults its form to them; `ResourceDef::form` arranges
/// or extends them instead, and a form renders the controls it does not place after the ones
/// it does. The table and detail page read the model, not the form: see
/// [`ModelColumns`](derive@crate::ModelColumns).
///
/// # Attributes
///
/// - `#[form(model = User)]` on the struct: the model the form writes.
/// - `#[form(blank = <expr>)]` on a scalar: the value an empty submission reads as.
/// - `#[form(optional)]` on a `String`: an empty submission reads as `""`.
/// - `#[form(options)]` on an `Options` enum or an `Option` of one: a choice over its options.
///   Without it, the field is a text field.
/// - `#[form(options = Status)]`: a choice over `Status::options()`.
/// - `#[form(relationship = AuthorResource)]` on a foreign key: a choice over the source's
///   records, each labelled by its `record_title`. The source is any `OptionSource`, which
///   every `Resource` is.
/// - `#[form(relationship = TagResource)]` on a `Vec` of the source's keys, named like the
///   model's `#[has_many(via = joins.target)]` field: a many-to-many field, a multiple choice
///   whose records the write links through the join model, one row each. No record is its
///   blank answer; an edit that does not post it keeps the record's links.
/// - `#[form(file)]` on a `String`: a file field.
/// - `#[form(embed)]` on an `EmbeddedForm` value.
/// - `#[form(repeat)]` on a `#[document]` list of [`FormInput`](derive@crate::FormInput)
///   items: a repeater, a row per item, which the browser adds, removes and moves. No rows is
///   its blank answer.
///
/// A scalar's **blank answer** is its `blank`, `""` for an `optional` `String`, `None` for an
/// `Option`, or `false` for a `bool`. A field with none is required: the panel renders its
/// control required and the parse refuses an empty submission. No control declares presence.
///
/// A generic struct, a tuple struct, an empty struct, a `Deferred<_>` field,
/// `blank` or `optional` on an `Option`, an embedded value or a many-to-many field, `optional`
/// on a type other than `String`, and an unknown key are compile errors. So are a field the
/// model lacks, a type the model's field does not have, a scalar that is not a `FormScalar`,
/// and a many-to-many field whose keys are not its source's primary key.
///
/// # Many-to-many fields
///
/// - The control is a multiple choice: a checkbox per record of the related resource, offered
///   as a relationship choice's options are, with the records the record links checked.
///   `.searchable()` filters the boxes as the user types; without JavaScript every box shows.
/// - The form posts the field's key once per checked box, after a hidden blank, so a
///   submission with no box checked links no record. Each key the record does not link yet
///   must name a record the choice offers, and the write checks each again inside its
///   transaction.
/// - A create links the new record to each chosen record, one join row each. An edit adds the
///   rows of the records it chose and deletes those of the ones it dropped; one that does not
///   post the field keeps the links. An overridden `create_record` or `update_record` links
///   only by delegating to `write_create` or `write_update`.
/// - An edit unlinks only the records its form could offer: a linked record the related
///   resource's policy hides from the user, or one in another tenant, stays linked.
/// - The choice offers at most 200 records. Past that it offers none, shows the ones the
///   record links, and an edit keeps or drops those but links no other; attach records from a
///   related table instead.
/// - A declared table or detail page lists the linked records by their titles with
///   `RelationColumn::list::<TagResource>(relation!(Article.tags))`.
/// - A new link sets the two keys and nothing else, so mounting refuses a join model with
///   another column that is neither nullable nor `#[auto]`, or a key spanning several columns.
///   A join model's `#[default]` and `#[update]` expressions are not applied: a nullable
///   column with one stays `NULL`.
/// - Making the two keys the join model's primary key, as `#[key(article_id, tag_id)]`, has
///   the database store each pair once, whatever two concurrent writes do. Mounting refuses a
///   multiple choice in a resource's form that names no many-to-many field. In an action's
///   input, [`parse_list`](crate::parse_list) reads a multiple choice.
///
/// # Submitting
///
/// - Validation runs in one round. The record form's parse (required fields and typed values),
///   email, relationships and uniqueness are checked together, and the form re-renders with
///   every error inline, with status 200 and nothing written. `validate_record` needs the
///   parsed form, so it runs only when every field parses. It returns
///   `FieldErrors<UserFormField>`, keyed by the record form's field enum
///   (`errors.add(UserFormField::Age, "Age must be zero or more")`), so every error renders
///   under its field's control, or an embedded value's first control. A rule that reads the
///   database refuses from `create_record` or `update_record` instead, which render their
///   [`WriteError::Refused`](crate::WriteError::Refused) errors the same way.
/// - Unknown keys are refused. A POST carrying a key the form does not declare answers 400, so
///   a client cannot write `role` or `tenant_id`. The CSRF token, the edit form's
///   `record_version` and the file fields' `clear_` and `keep_` keys are the exceptions.
/// - An edit writes only what was posted. On an edit, a declared key missing from the
///   submission keeps its stored value, and the update assigns only the fields the submission
///   named, plus the model's own `#[update(..)]` defaults and `#[version]` bump. An API client
///   can post a single field. A control submitted empty is posted, and stores its field's
///   blank value.
/// - A save another save overtook is refused. When the model has a `#[version]` field and
///   derives `ModelColumns`, the edit form carries the version it opened on as
///   `record_version`. A save whose version no longer matches the stored record writes nothing
///   and re-renders the form with the submitted values, an alert above them, and the current
///   version, so saving again overwrites the other save. A save that lands between the edit's
///   load and its write fails Toasty's version condition and re-renders the same way where the
///   database reports it so, as PostgreSQL and MySQL do; SQLite can refuse the transaction as
///   busy instead, and Turso's concurrent transactions conflict at commit, which both answer
///   500. A POST without `record_version` skips the first comparison, not the second; one
///   whose `record_version` is no number answers 400. Only the edit form carries the version:
///   actions, relation managers and the profile pages write without it, and the default Delete
///   removes the record whatever its version. Mounting refuses a form field named
///   `record_version` or `csrf_token`.
pub use tablo_macros::RecordForm;
