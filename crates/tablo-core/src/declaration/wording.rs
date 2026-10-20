//! The wording of each [`DeclarationErrorKind`].

use std::fmt;

use super::{DeclarationErrorKind, fmt_action_input, fmt_field_rule, fmt_many_to_many};

impl fmt::Display for DeclarationErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDb => f.write_str(
                "the router holds no Db: install it with `.app_context(db)` before mounting the \
                 panel",
            ),
            Self::MissingAuthModels { models } => write!(
                f,
                "auth is on, but the Db does not register its shipped models ({models}): build \
                 the Db from `tablo::models!(…)`, which registers them, or opt out with \
                 `.auth(Auth::disabled())`",
                models = models.join(", ")
            ),
            Self::ShellAssetsWithoutBundle => f.write_str(
                "shell_assets need the router's asset bundle: install it with `.assets(..)` \
                 before mounting the panel",
            ),
            Self::NotificationsWithoutAuth => f.write_str(
                "notifications are for signed-in users: `Panel::notifications` needs auth on",
            ),
            Self::FileFieldWithoutUploader { field } => write!(
                f,
                "file field `{field}` needs an uploader to store its bytes: install one with \
                 `Panel::uploads`"
            ),
            Self::InvalidSegment {
                item,
                segment,
                fault,
            } => write!(f, "{item} '{segment}' {fault}"),
            Self::PrefixOverlapsRuntime { prefix } => write!(
                f,
                "prefix '{prefix}' overlaps Topcoat's runtime endpoints at '{}'",
                crate::topcoat_compat::RUNTIME_PREFIX
            ),
            Self::PrefixOverlapsPanel { other } => write!(
                f,
                "the prefix overlaps the panel mounted at '{other}': each panel needs a prefix of \
                 its own"
            ),
            Self::EndpointOutsideRuntime { path } => write!(
                f,
                "endpoint '{path}' is not a shard or procedure under '{}': the panel's prefix or \
                 the app's own layers guard a route elsewhere",
                crate::topcoat_compat::RUNTIME_PREFIX
            ),
            Self::ServeDirWithoutCatchAll { path } => write!(
                f,
                "serve_dir path '{path}' must end in a catch-all like '/uploads/{{*file}}'"
            ),
            Self::ServeDirTwice { path } => write!(f, "serve_dir path '{path}' is declared twice"),
            Self::ServeDirTaken { path, panel } => write!(
                f,
                "serve_dir path '{path}' is already served by the panel mounted at '{panel}'"
            ),
            Self::SecondHome => {
                f.write_str("a home page is already registered: `Panel::home` takes one")
            }
            Self::DuplicateResource => f.write_str(
                "registered twice: a panel mounts each resource type once, so mount a variant \
                 under its own type",
            ),
            Self::DuplicatePage => f.write_str(
                "registered twice: a panel mounts each page type once, so mount a variant under \
                 its own type",
            ),
            Self::NotMounted => f.write_str(
                "not mounted on the context's panel: register it with `Panel::resource`",
            ),
            Self::ReservedSlug { slug } => {
                write!(f, "slug '{slug}' names a route the panel serves itself")
            }
            Self::DuplicateSlug { slug } => write!(
                f,
                "slug '{slug}' is taken by another resource or page: each needs a distinct `slug()`"
            ),
            Self::DuplicateAction { name } => write!(
                f,
                "two actions are named '{name}': each needs a distinct `NAME`"
            ),
            Self::ActionInput { action, fault } => fmt_action_input(f, action, fault),
            Self::SignUp(fault) => fault.fmt(f),
            Self::Profile(fault) => fault.fmt(f),
            Self::PasswordReset(fault) => fault.fmt(f),
            Self::EmailVerification(fault) => fault.fmt(f),
            Self::TenantRegistration(fault) => fault.fmt(f),
            Self::TenantProfile(fault) => fault.fmt(f),
            Self::TwoFactor(fault) => fault.fmt(f),
            Self::AuthMail(fault) => fault.fmt(f),
            Self::DuplicateRelation => {
                f.write_str("declared twice: each related resource is one relation")
            }
            Self::UnregisteredRelation => f.write_str(
                "the related resource is not registered on this panel: declare it with \
                 `Panel::resource`",
            ),
            Self::UnregisteredOptionSource { field, source } => write!(
                f,
                "field '{field}' takes its options from `{source}`, which this panel does not \
                 register: declare it with `Panel::resource`"
            ),
            Self::UnregisteredLabelSource { column, source } => write!(
                f,
                "column '{column}' labels its records by `{source}`, which this panel does not \
                 register: declare it with `Panel::resource`"
            ),
            Self::TraversalLens { steps } => write!(
                f,
                "a lens here names one field of its model, not a {steps}-step path"
            ),
            Self::UnresolvedLens { model, steps } => write!(
                f,
                "lens path {steps:?} resolves to no single column of `{model}`: only embedded \
                 steps (embedded structs, enum variant fields and `#[document]` fields) bind, not \
                 relation hops"
            ),
            Self::Uncompiled { model, error } => write!(
                f,
                "`{model}` compiles to no schema, so its embedded paths name no column: {error}"
            ),
            Self::Tenancy { fault } => fault.fmt(f),
            Self::NoColumns => f.write_str(
                "no column: declare columns with `Table::new(columns)` or in `ResourceDef::table`",
            ),
            Self::ZeroPageSize => f.write_str("`Table::paginate` needs a page size of at least 1"),
            Self::UnsortableDefaultSort { column } => write!(
                f,
                "`Table::default_sort` sorts on '{column}', but the table declares no sortable column for it"
            ),
            Self::DuplicateColumn { name } => write!(
                f,
                "two columns are named '{name}': each column needs a distinct name"
            ),
            Self::DuplicateFilter { name } => write!(
                f,
                "two filters are named '{name}': each filter needs a distinct name"
            ),
            Self::DottedFilterName { name } => write!(f, "filter name '{name}' holds a '.'"),
            Self::UnparsedFilterOption { filter, value } => write!(
                f,
                "filter '{filter}' offers '{value}', which its field's type does not parse: spell \
                 each option as the field's form value, or take the type's own with \
                 `SelectFilter::of`"
            ),
            Self::DuplicateField { name } => write!(
                f,
                "two fields are named '{name}': each input needs a distinct field"
            ),
            Self::DisabledWithoutValue { field } => write!(
                f,
                "field '{field}' is disabled and required, so no create can fill it: give it a \
                 `default` (a file field takes none), make it optional, or use `disabled_on_edit`"
            ),
            Self::RepeaterLeafModifier { field, leaf } => write!(
                f,
                "repeater '{field}' disables its row's '{leaf}' or gives it a default, which a \
                 row does not apply: disable the repeater's own field instead"
            ),
            Self::RepeaterPassword { field, leaf } => write!(
                f,
                "repeater '{field}' rows hold password '{leaf}', which an edit never renders \
                 back, so each save overwrites it: keep the secret out of the item"
            ),
            Self::FieldRule { field, fault } => fmt_field_rule(f, field, fault),
            Self::UnpostedDefault { field, value } => write!(
                f,
                "field '{field}' defaults to '{value}', which its control never posts: spell the \
                 default as the field's form value, a list for a multiple choice"
            ),
            Self::UnplacedWatchedField { field } => write!(
                f,
                "a condition watches field '{field}', which the schema does not place: place it \
                 in the same schema"
            ),
            Self::HiddenWatchedField { field } => write!(
                f,
                "a condition watches field '{field}', which a condition hides while the guarded \
                 field or block still shows: place the guarded one inside the block that hides \
                 '{field}'"
            ),
            Self::UnpostedConditionValue { field, value } => write!(
                f,
                "a condition shows a field while '{field}' posts '{value}', which it never posts: \
                 name one of its option values, or `true` or `false` for a toggle"
            ),
            Self::MisdeclaredDependentChoice { field } => write!(
                f,
                "choice '{field}' depends on a column its options do not have: call \
                 `relationship::<R>()` and name a column of `R`'s model in `depends_on`"
            ),
            Self::MultipleDependentChoice { field } => write!(
                f,
                "choice '{field}' depends on another field through a multiple choice, which \
                 neither narrows another choice nor is narrowed: drop `depends_on` or `multiple`"
            ),
            Self::UnplacedParentField { field, parent } => write!(
                f,
                "choice '{field}' depends on field '{parent}', which the schema does not place: \
                 place it in the same schema"
            ),
            Self::RequiredConditionalField { field } => write!(
                f,
                "field '{field}' is conditional but has no blank answer: a submission that hides \
                 it posts nothing, so make it an `Option` or give it `#[form(blank = ..)]`"
            ),
            Self::MissingControl { field, key } => write!(
                f,
                "record-form field `{field}` binds key `{key}`, but no control posts it: \
                 `RecordForm::control` must render the field's control"
            ),
            Self::FormClaimsTenantColumn { field, column } => write!(
                f,
                "record-form field `{field}` claims the tenant column `{column}`, which the \
                 framework stamps on create: drop it from the form"
            ),
            Self::ReservedFormField { field } => write!(
                f,
                "form field `{field}` takes a name the form's POST carries itself, so the two \
                 would read each other's value: rename the field"
            ),
            Self::EmptyChoice { field } => write!(
                f,
                "choice field `{field}` declares no options and no relationship, so it offers \
                 nothing to choose: add `.options(..)` or `.relationship::<Source>()`, or \
                 `#[form(relationship = Source)]` on a record form's field"
            ),
            Self::CompositeKeyChoice { field } => write!(
                f,
                "`{field}` reads a relationship whose model has a composite primary key, which \
                 no option value can spell"
            ),
            Self::UniqueWithoutIndex { field } => write!(
                f,
                "field `{field}` is marked unique, but no unique index covers its column: add \
                 `#[unique]` (or `#[unique(..)]`) to the model or drop `.unique()`, which would \
                 otherwise check a rule the database does not enforce"
            ),
            Self::OptionalUnique { field } => write!(
                f,
                "field `{field}` is marked unique, but its record-form field answers an empty \
                 submission and its column is not nullable, so every empty submission stores \
                 the same value: make the field required, or an `Option`"
            ),
            Self::CreateColumnsNameTenant { column } => write!(
                f,
                "`create_column` names the tenant column `{column}`, which the framework stamps \
                 on create: drop it there and delegate to `write_create`"
            ),
            Self::UnwrittenColumn { column } => write!(
                f,
                "nothing writes the non-nullable column `{column}` on create: the record form has \
                 no such field, toasty fills no `#[default(..)]` for it, and no `create_column` \
                 names it, so every create would fail at the driver; a resource that creates \
                 nothing declares `edit_only`"
            ),
            Self::ManyToMany { field, fault } => fmt_many_to_many(f, field, fault),
        }
    }
}
