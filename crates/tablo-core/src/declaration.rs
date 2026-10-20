//! The declaration mistakes a panel refuses to mount with, and the one place their wording lives.

use std::fmt;

mod action_input;
mod auth;
mod field_rule;
mod many_to_many;
mod mount_error;
mod segment;
mod tenancy;
mod wording;

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
                Site::SoftDelete => f.write_str(" soft delete")?,
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
    /// `ResourceDef::soft_delete`.
    SoftDelete,
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
    /// A soft-deleting resource's record form claims the soft-delete column, which only a delete
    /// and a restore write.
    FormClaimsSoftDeleteColumn {
        /// The record-form field.
        field: String,
        /// The soft-delete column.
        column: String,
    },
    /// A form field named after a key the form's POST carries itself, such as the CSRF token
    /// or the edit form's `record_version`.
    ReservedFormField {
        /// The field.
        field: String,
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

#[cfg(test)]
mod tests;
