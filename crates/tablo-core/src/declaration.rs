//! The declaration mistakes a panel refuses to mount with, and the one place their wording lives.

use std::fmt;

mod action_input;
mod auth;
mod field_rule;
mod many_to_many;
mod mount_error;
mod segment;
mod tenancy;

pub use action_input::ActionInputFault;
use action_input::fmt_action_input;
pub use auth::{
    AuthMailFault, ProfileFault, ResetFault, SignUpFault, TenantProfileFault,
    TenantRegistrationFault, TwoFactorFault, VerificationFault,
};
pub use field_rule::FieldRuleFault;
use field_rule::fmt_field_rule;
pub use many_to_many::ManyToManyFault;
use many_to_many::fmt_many_to_many;
pub use mount_error::MountError;
pub use segment::SegmentFault;
pub(crate) use segment::segment_fault;
pub use tenancy::TenancyFault;

/// One declaration mistake: who declares it, where, and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct DeclarationError {
    /// The resource or page type that declares it; `None` for the panel's own configuration.
    pub resource: Option<&'static str>,
    /// The part of the declaration it is in.
    pub site: Site,
    /// What is wrong.
    pub kind: DeclarationErrorKind,
}

impl DeclarationError {
    /// A mistake in the panel's own configuration.
    pub(crate) fn panel(kind: DeclarationErrorKind) -> Self {
        Self {
            resource: None,
            site: Site::Panel,
            kind,
        }
    }

    /// A mistake in `T`'s declaration.
    pub(crate) fn of<T>(site: Site, kind: DeclarationErrorKind) -> Self {
        Self {
            resource: Some(std::any::type_name::<T>()),
            site,
            kind,
        }
    }
}

impl fmt::Display for DeclarationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(resource) = self.resource {
            write!(f, "`{resource}`")?;
            match &self.site {
                Site::Panel | Site::Registration => {}
                Site::Tenancy => f.write_str(" tenancy")?,
                Site::Table => f.write_str(" table")?,
                Site::Form => f.write_str(" form")?,
                Site::Detail => f.write_str(" detail")?,
                Site::Relation(key) => write!(f, " relation `{key}`")?,
            }
            f.write_str(": ")?;
        }
        write!(f, "{}", self.kind)
    }
}

impl std::error::Error for DeclarationError {}

/// The part of a declaration a [`DeclarationError`] is in.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Site {
    /// The panel's own configuration: its prefix, served directories, assets and auth.
    Panel,
    /// The resource or page itself: its registration, slug, actions or create columns, or a
    /// second home page.
    Registration,
    /// `ResourceDef::tenancy`.
    Tenancy,
    /// `ResourceDef::table`.
    Table,
    /// `ResourceDef::form`, checked against the record form.
    Form,
    /// `ResourceDef::detail`.
    Detail,
    /// The `ResourceDef::relation` to the resource with this slug, or this type when the panel
    /// does not register it.
    Relation(String),
}

/// What is wrong with a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeclarationErrorKind {
    /// The router holds no `toasty::Db`.
    MissingDb,
    /// Auth is on, but the `Db` does not register the shipped models it needs.
    MissingAuthModels {
        /// The unregistered models.
        models: Vec<&'static str>,
    },
    /// `Panel::shell_assets` is set, but the router has no asset bundle.
    ShellAssetsWithoutBundle,
    /// `Panel::notifications` on `Auth::disabled`, which signs nobody in to notify.
    NotificationsWithoutAuth,
    /// A form declares a file field, but the panel installs no `Uploader` to store its bytes.
    FileFieldWithoutUploader {
        /// The file field's name.
        field: String,
    },
    /// A path segment the panel routes cannot be a literal URL segment.
    InvalidSegment {
        /// What names the segment: `panel prefix`, `ResourceDef::slug` or `PageDef::slug`.
        item: &'static str,
        /// The refused segment.
        segment: String,
        /// Why it is refused.
        fault: SegmentFault,
    },
    /// The prefix overlaps Topcoat's runtime endpoints.
    PrefixOverlapsRuntime {
        /// The panel's prefix.
        prefix: String,
    },
    /// The prefix overlaps another panel's.
    PrefixOverlapsPanel {
        /// The other panel's prefix.
        other: String,
    },
    /// A `Panel::endpoint` route outside Topcoat's runtime endpoints.
    EndpointOutsideRuntime {
        /// The route's path.
        path: String,
    },
    /// A `Panel::serve_dir` pattern that does not end in a catch-all.
    ServeDirWithoutCatchAll {
        /// The pattern.
        path: String,
    },
    /// The panel serves the same directory pattern twice.
    ServeDirTwice {
        /// The pattern.
        path: String,
    },
    /// Another panel already serves the directory pattern.
    ServeDirTaken {
        /// The pattern.
        path: String,
        /// The serving panel's prefix.
        panel: String,
    },
    /// A second `Panel::home` page.
    SecondHome,
    /// The resource is registered on the panel twice.
    DuplicateResource,
    /// The page is registered on the panel twice, `Panel::home` included.
    DuplicatePage,
    /// The resource is not mounted on the context's panel.
    NotMounted,
    /// A slug naming a route the panel serves itself.
    ReservedSlug {
        /// The slug.
        slug: String,
    },
    /// A slug another resource or page already mounts at.
    DuplicateSlug {
        /// The slug.
        slug: String,
    },
    /// Two actions share a `NAME`.
    DuplicateAction {
        /// The shared name.
        name: &'static str,
    },
    /// An action's input cannot be served.
    ActionInput {
        /// The action's `NAME`.
        action: &'static str,
        /// What is wrong with it.
        fault: ActionInputFault,
    },
    /// The panel's sign-up page cannot be served.
    SignUp(SignUpFault),
    /// The panel's profile page cannot be served.
    Profile(ProfileFault),
    /// The panel's password reset cannot be served.
    PasswordReset(ResetFault),
    /// The panel's email verification cannot be served.
    EmailVerification(VerificationFault),
    /// The panel's tenant registration page cannot be served.
    TenantRegistration(TenantRegistrationFault),
    /// The panel's tenant settings page cannot be served.
    TenantProfile(TenantProfileFault),
    /// The panel's two-factor sign-in cannot be served.
    TwoFactor(TwoFactorFault),
    /// The panel's auth mail cannot be sent.
    AuthMail(AuthMailFault),
    /// Two relations to the same resource.
    DuplicateRelation,
    /// A relation to a resource the panel does not register.
    UnregisteredRelation,
    /// A relationship field, or a relationship filter, whose options come from a resource the
    /// panel does not register.
    UnregisteredOptionSource {
        /// The field.
        field: String,
        /// The option source's type.
        source: &'static str,
    },
    /// A relation column labelling its records by a resource the panel does not register.
    UnregisteredLabelSource {
        /// The column.
        column: String,
        /// The label source's type.
        source: &'static str,
    },
    /// A lens with more than one step where a single field of the model is needed.
    TraversalLens {
        /// The lens's step count.
        steps: usize,
    },
    /// A multi-step lens that resolves to no single column.
    UnresolvedLens {
        /// The lens's model.
        model: &'static str,
        /// The lens's field indices, step by step.
        steps: Vec<usize>,
    },
    /// A model whose schema no driver compiles, so its embedded paths name no column.
    Uncompiled {
        /// The model.
        model: &'static str,
        /// Each driver's reason to refuse it.
        error: String,
    },
    /// A resource's tenancy that cannot scope its rows.
    Tenancy {
        /// What is wrong.
        fault: TenancyFault,
    },
    /// A table with no column.
    NoColumns,
    /// `Table::paginate(0)`.
    ZeroPageSize,
    /// `Table::default_sort` names no sortable column.
    UnsortableDefaultSort {
        /// The named column.
        column: String,
    },
    /// Two table columns share a name.
    DuplicateColumn {
        /// The shared name.
        name: String,
    },
    /// Two table filters share a name.
    DuplicateFilter {
        /// The shared name.
        name: String,
    },
    /// A table filter's name holds a `.`, which its URL parameters reserve for a part:
    /// `f.<name>.<part>`.
    DottedFilterName {
        /// The name.
        name: String,
    },
    /// A select filter option whose value the field's type does not parse, so choosing it
    /// filters nothing.
    UnparsedFilterOption {
        /// The filter's name.
        filter: String,
        /// The option's value.
        value: String,
    },
    /// Two schema fields share a name.
    DuplicateField {
        /// The shared name.
        name: String,
    },
    /// A field disabled on the create form, with no `default` and no blank answer: no create
    /// could fill it.
    DisabledWithoutValue {
        /// The disabled field.
        field: String,
    },
    /// A repeater's item disables one of its row's controls or gives it a default, which a row
    /// does not apply.
    RepeaterLeafModifier {
        /// The repeater.
        field: String,
        /// The control in its row.
        leaf: String,
    },
    /// A repeater's item has a password control, which an edit cannot render back, so every save
    /// would overwrite the stored secret.
    RepeaterPassword {
        /// The repeater.
        field: String,
        /// The password control in its row.
        leaf: String,
    },
    /// A text field's declared length, bounds or pattern cannot work.
    FieldRule {
        /// The field, or `repeater.leaf` for a control in a repeater's rows.
        field: String,
        /// What is wrong.
        fault: FieldRuleFault,
    },
    /// A field's default is a value its control never posts: one its type does not parse, one
    /// not among a choice's options, or a list or one value where the control posts the other.
    UnpostedDefault {
        /// The field.
        field: String,
        /// The default, a list spelled `["a", "b"]`.
        value: String,
    },
    /// A condition watches a field the schema does not place.
    UnplacedWatchedField {
        /// The watched field.
        field: String,
    },
    /// A condition watches a field that another condition hides while what it guards still
    /// shows.
    HiddenWatchedField {
        /// The watched field.
        field: String,
    },
    /// A condition names a value its watched field never posts: not one of a choice's options, or
    /// neither `true` nor `false` for a toggle.
    UnpostedConditionValue {
        /// The watched field.
        field: String,
        /// The value.
        value: String,
    },
    /// A dependent choice's column belongs to another model than its relationship's source, or
    /// the choice has no relationship.
    MisdeclaredDependentChoice {
        /// The dependent choice.
        field: String,
    },
    /// A dependent choice that takes several options, or that depends on one that does: a
    /// multiple choice's key holds a list, which narrows nothing.
    MultipleDependentChoice {
        /// The dependent choice.
        field: String,
    },
    /// A dependent choice depends on a field the schema does not place.
    UnplacedParentField {
        /// The dependent choice.
        field: String,
        /// The field it depends on.
        parent: String,
    },
    /// A field a condition hides has no blank answer, so a submission hiding it cannot parse.
    RequiredConditionalField {
        /// The field.
        field: String,
    },
    /// A record-form field binding a key no control posts, after the form renders every field
    /// it does not place: a hand-written [`RecordForm::control`](crate::RecordForm::control)
    /// that renders no control for it.
    MissingControl {
        /// The record-form field.
        field: String,
        /// The key it binds.
        key: String,
    },
    /// A tenant-scoped resource's record form claims the tenant column.
    FormClaimsTenantColumn {
        /// The record-form field.
        field: String,
        /// The tenant column.
        column: String,
    },
    /// A choice field with neither options nor a relationship.
    EmptyChoice {
        /// The field.
        field: String,
    },
    /// A relationship field, or a relationship filter, over a model with a composite primary key.
    CompositeKeyChoice {
        /// The field, or the filter's name.
        field: String,
    },
    /// A field marked unique over a column no unique index covers.
    UniqueWithoutIndex {
        /// The field.
        field: String,
    },
    /// A unique field whose non-nullable column stores one value for every empty submission.
    OptionalUnique {
        /// The field.
        field: String,
    },
    /// [`create_column`](crate::ResourceDef::create_column) names the tenant column.
    CreateColumnsNameTenant {
        /// The tenant column.
        column: String,
    },
    /// A non-nullable column nothing writes on create, for a resource that serves a create page.
    UnwrittenColumn {
        /// The column.
        column: String,
    },
    /// A many-to-many field or relation whose join model cannot link records.
    ManyToMany {
        /// The field, or the relation's `via` field.
        field: String,
        /// What is wrong.
        fault: ManyToManyFault,
    },
}

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

#[cfg(test)]
mod tests;
