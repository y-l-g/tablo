//! The declaration mistakes a panel refuses to mount with, and the one place their wording lives.

use std::fmt;

mod action_input;
mod auth;
mod field_access;
mod field_rule;
mod locale;
mod many_to_many;
mod mount_error;
mod segment;
mod site;
mod tenancy;
mod wording;

pub use action_input::ActionInputFault;
use action_input::fmt_action_input;
pub use auth::{
    AuthMailFault, ProfileFault, ResetFault, SignUpFault, TenantProfileFault,
    TenantRegistrationFault, TwoFactorFault, VerificationFault,
};
pub use field_access::FieldAccessFault;
pub use field_rule::FieldRuleFault;
use field_rule::fmt_field_rule;
pub use locale::LocaleFault;
pub use many_to_many::ManyToManyFault;
use many_to_many::fmt_many_to_many;
pub use mount_error::MountError;
pub use segment::SegmentFault;
pub(crate) use segment::segment_fault;
pub use site::Site;
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
                Site::FieldAccess(field) => write!(f, " field access `{field}`")?,
                Site::RecordTitle => f.write_str(" record title")?,
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

/// What is wrong with a declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DeclarationErrorKind {
    /// The router holds no `toasty::Db`.
    ///
    /// Call `.with(db)` before `.panel(..)`.
    MissingDb,
    /// Auth is on, but the `Db` does not register the shipped models it needs: `AuthSession`,
    /// `AdminUser` for the shipped authenticator, `ColumnPreference` while a list has a
    /// toggleable column, `AuthToken` for a feature that mails links or two-factor sign-in, and
    /// `AuthFactor` for two-factor sign-in.
    ///
    /// Build the `Db` from `tablo::models!(..)`, add them to `toasty::models!(..)`, or opt out
    /// with `.auth(Auth::disabled())`.
    MissingAuthModels {
        /// The unregistered models.
        models: Vec<&'static str>,
    },
    /// `Panel::shell_assets` is set, but the router has no asset bundle.
    ///
    /// Call `.assets(..)` before `.panel(..)`.
    ShellAssetsWithoutBundle,
    /// `Panel::notifications` on `Auth::disabled`, which signs nobody in to notify.
    NotificationsWithoutAuth,
    /// A form declares a file field, but the panel installs no `Uploader` to store its bytes.
    ///
    /// Install one with `Panel::uploads` or `Panel::uploads_dir`.
    FileFieldWithoutUploader {
        /// The file field's name.
        field: String,
    },
    /// A path segment the panel routes cannot be a literal URL segment.
    ///
    /// Pick a plain segment such as `blog-posts`.
    InvalidSegment {
        /// What names the segment: `panel prefix`, `ResourceDef::slug` or `PageDef::slug`.
        item: &'static str,
        /// The refused segment.
        segment: String,
        /// Why it is refused.
        fault: SegmentFault,
    },
    /// The prefix overlaps Topcoat's runtime endpoints.
    ///
    /// Pick another prefix.
    PrefixOverlapsRuntime {
        /// The panel's prefix.
        prefix: String,
    },
    /// The prefix overlaps another panel's.
    ///
    /// Give each panel a prefix of its own.
    PrefixOverlapsPanel {
        /// The other panel's prefix.
        other: String,
    },
    /// A `Panel::endpoint` route outside Topcoat's runtime endpoints.
    ///
    /// Declare only shards and procedures; the panel's prefix guards its other routes.
    EndpointOutsideRuntime {
        /// The route's path.
        path: String,
    },
    /// A `Panel::serve_dir` pattern that does not end in a catch-all.
    ///
    /// End it in `{*file}`: `/uploads/{*file}`.
    ServeDirWithoutCatchAll {
        /// The pattern.
        path: String,
    },
    /// The panel serves the same directory pattern twice.
    ///
    /// Drop the second `serve_dir`.
    ServeDirTwice {
        /// The pattern.
        path: String,
    },
    /// Another panel already serves the directory pattern.
    ///
    /// Serve it from one panel.
    ServeDirTaken {
        /// The pattern.
        path: String,
        /// The serving panel's prefix.
        panel: String,
    },
    /// A `Panel::content_security_policy` that is no valid header value: drop its line breaks.
    InvalidContentSecurityPolicy {
        /// The policy.
        policy: String,
    },
    /// A second `Panel::home` page.
    ///
    /// Keep one home page; register the other with `page`.
    SecondHome,
    /// The resource is registered on the panel twice.
    ///
    /// Register it once; mount a variant under its own type.
    DuplicateResource,
    /// The page is registered on the panel twice, `Panel::home` included.
    ///
    /// Register it once; mount a variant under its own type.
    DuplicatePage,
    /// The resource is not mounted on the context's panel.
    ///
    /// It refuses no mount: a request answers 500 when app code asks for a resource its panel
    /// does not register, through [`wired_table`](crate::wired_table),
    /// [`scoped_query`](crate::scoped_query), [`write_create`](crate::write_create) or
    /// [`write_update`](crate::write_update). Register the resource on that panel, or reach it
    /// through a context of a panel that does.
    NotMounted,
    /// A slug naming a route the panel serves itself: `login`, `logout`, `register`, `profile`,
    /// `forgot-password`, `reset-password` or `-`.
    ///
    /// Rename it with `ResourceDef::slug` or `PageDef::slug`.
    ReservedSlug {
        /// The slug.
        slug: String,
    },
    /// A slug another resource or page already mounts at.
    ///
    /// Give each a distinct slug.
    DuplicateSlug {
        /// The slug.
        slug: String,
    },
    /// Two actions share a `NAME`.
    ///
    /// Rename one.
    DuplicateAction {
        /// The shared name.
        name: &'static str,
    },
    /// Two widgets of a page, or of a resource's list, share a name.
    ///
    /// Give one a distinct [`WidgetDef::name`](crate::WidgetDef::name).
    DuplicateWidget {
        /// The shared name.
        name: String,
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
    /// The panel's locales or translations cannot be served.
    Locale(LocaleFault),
    /// Two relations to the same resource.
    ///
    /// Keep one.
    DuplicateRelation,
    /// A relation to a resource the panel does not register.
    ///
    /// Register the related resource.
    UnregisteredRelation,
    /// A relationship field, or a relationship filter, whose options come from a resource the
    /// panel does not register.
    ///
    /// Register that resource.
    UnregisteredOptionSource {
        /// The field.
        field: String,
        /// The option source's type.
        source: &'static str,
    },
    /// A relation column labelling its records by a resource the panel does not register:
    /// `RelationColumn::{new, of, list}::<R>` or `CountColumn::of::<R>`.
    ///
    /// Register `R`, or use `RelationColumn::unscoped` for a model no resource administers.
    UnregisteredLabelSource {
        /// The column.
        column: String,
        /// The label source's type.
        source: &'static str,
    },
    /// A lens with more than one step where a single field of the model is needed.
    ///
    /// Name one field of the model.
    TraversalLens {
        /// The lens's step count.
        steps: usize,
    },
    /// A multi-step lens that resolves to no single column.
    ///
    /// Step only through embedded structs, enum variant fields and `#[document]` fields.
    UnresolvedLens {
        /// The lens's model.
        model: &'static str,
        /// The lens's field indices, step by step.
        steps: Vec<usize>,
    },
    /// A model whose schema no driver compiles, so its embedded paths name no column. The error
    /// gives the reason of each of Toasty's PostgreSQL, SQLite, MySQL and DynamoDB drivers.
    ///
    /// Fix the model for your driver.
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
    /// A field access that cannot hide its field everywhere the panel shows it.
    FieldAccess {
        /// What is wrong.
        fault: FieldAccessFault,
    },
    /// A table with no column.
    ///
    /// Declare columns; a resource whose model's table lists nothing declares its own `table`.
    NoColumns,
    /// `Table::paginate(0)`.
    ///
    /// Pass a page size of at least 1.
    ZeroPageSize,
    /// `Table::default_sort` names no sortable column.
    ///
    /// Declare its column `.sortable()`.
    UnsortableDefaultSort {
        /// The named column.
        column: String,
    },
    /// Two table columns share a name.
    ///
    /// Rename one.
    DuplicateColumn {
        /// The shared name.
        name: String,
    },
    /// Two table filters share a name.
    ///
    /// Rename one.
    DuplicateFilter {
        /// The shared name.
        name: String,
    },
    /// A table filter's name holds a `.`, which its URL parameters reserve for a part:
    /// `f.<name>.<part>`.
    ///
    /// Rename it without the `.`.
    DottedFilterName {
        /// The name.
        name: String,
    },
    /// A select filter option whose value the field's type does not parse, so choosing it
    /// filters nothing.
    ///
    /// Spell it as the field's form value, or build the filter with `SelectFilter::of`.
    UnparsedFilterOption {
        /// The filter's name.
        filter: String,
        /// The option's value.
        value: String,
    },
    /// Two schema fields share a name.
    ///
    /// Bind each input to a distinct field.
    DuplicateField {
        /// The shared name.
        name: String,
    },
    /// A field disabled on the create form, with no `default` and no blank answer: no create
    /// could fill it.
    ///
    /// Give it a `default`, make it optional, or use `disabled_on_edit`.
    DisabledWithoutValue {
        /// The disabled field.
        field: String,
    },
    /// A repeater's item disables one of its row's controls or gives it a default, which a row
    /// does not apply.
    ///
    /// Disable the repeater's own field.
    RepeaterLeafModifier {
        /// The repeater.
        field: String,
        /// The control in its row.
        leaf: String,
    },
    /// A repeater's item has a password control, which an edit cannot render back, so every save
    /// would overwrite the stored secret.
    ///
    /// Keep the secret out of the repeater's item.
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
    ///
    /// Spell the default as the field's form value, a list for a multiple choice.
    UnpostedDefault {
        /// The field.
        field: String,
        /// The default, a list spelled `["a", "b"]`.
        value: String,
    },
    /// A condition watches a field the schema does not place.
    ///
    /// Place the watched field in the same schema.
    UnplacedWatchedField {
        /// The watched field.
        field: String,
    },
    /// A condition watches a field that another condition hides while what it guards still
    /// shows.
    ///
    /// Place the guarded field inside the block that hides the watched one.
    HiddenWatchedField {
        /// The watched field.
        field: String,
    },
    /// A condition names a value its watched field never posts: not one of a choice's options, or
    /// neither `true` nor `false` for a toggle.
    ///
    /// Name one of its options, or `true` or `false` for a toggle.
    UnpostedConditionValue {
        /// The watched field.
        field: String,
        /// The value.
        value: String,
    },
    /// A dependent choice's column belongs to another model than its relationship's source, or
    /// the choice has no relationship.
    ///
    /// Declare `relationship::<R>()`, and name a column of `R`'s model in `depends_on`.
    MisdeclaredDependentChoice {
        /// The dependent choice.
        field: String,
    },
    /// A dependent choice that takes several options, or that depends on one that does: a
    /// multiple choice's key holds a list, which narrows nothing.
    ///
    /// Drop `depends_on` or `multiple`.
    MultipleDependentChoice {
        /// The dependent choice.
        field: String,
    },
    /// A dependent choice rendered as [radios](crate::ChoiceField::radio) or
    /// [buttons](crate::ChoiceField::buttons): the browser lists its options once, so it cannot
    /// narrow them when the parent changes.
    ///
    /// Drop `depends_on`, or `radio` or `buttons`.
    ListedDependentChoice {
        /// The dependent choice.
        field: String,
    },
    /// A dependent choice depends on a field the schema does not place.
    ///
    /// Place that field in the same schema.
    UnplacedParentField {
        /// The dependent choice.
        field: String,
        /// The field it depends on.
        parent: String,
    },
    /// A field a condition hides has no blank answer, so a submission hiding it cannot parse.
    ///
    /// Make it an `Option`, or give it `#[form(blank = ..)]`.
    RequiredConditionalField {
        /// The field.
        field: String,
    },
    /// A record-form field binding a key no control posts, after the form renders every field
    /// it does not place: a hand-written [`RecordForm::control`](crate::RecordForm::control)
    /// that renders no control for it.
    ///
    /// Render the field's control.
    MissingControl {
        /// The record-form field.
        field: String,
        /// The key it binds.
        key: String,
    },
    /// A tenant-scoped resource's record form claims the tenant column.
    ///
    /// Drop the field: the framework stamps the tenant.
    FormClaimsTenantColumn {
        /// The record-form field.
        field: String,
        /// The tenant column.
        column: String,
    },
    /// A soft-deleting resource's record form claims the soft-delete column, which only a delete
    /// and a restore write.
    ///
    /// Drop the field, or mark it `#[form(skip)]` on a `ModelForm`: only a delete and a restore
    /// write the column.
    FormClaimsSoftDeleteColumn {
        /// The record-form field.
        field: String,
        /// The soft-delete column.
        column: String,
    },
    /// A form field named after a key the form's POST carries itself: the CSRF token, the edit
    /// form's `record_version`, or the create form's `create_another`.
    ReservedFormField {
        /// The field.
        field: String,
    },
    /// A choice field with neither options nor a relationship.
    ///
    /// Declare `.options(..)`, `.relationship::<R>()`, or `#[form(relationship = R)]`.
    EmptyChoice {
        /// The field.
        field: String,
    },
    /// A relationship field, or a relationship filter, over a model with a composite primary key.
    ///
    /// Choose over a model with a single-column key.
    CompositeKeyChoice {
        /// The field, or the filter's name.
        field: String,
    },
    /// A field marked unique over a column no unique index covers.
    ///
    /// Add `#[unique]` to the model, or drop `.unique()`.
    UniqueWithoutIndex {
        /// The field.
        field: String,
    },
    /// A unique field whose non-nullable column stores one value for every empty submission.
    ///
    /// Make the field required, or an `Option`.
    OptionalUnique {
        /// The field.
        field: String,
    },
    /// [`create_column`](crate::ResourceDef::create_column) names the tenant column.
    ///
    /// Drop it: the framework stamps the tenant.
    CreateColumnsNameTenant {
        /// The tenant column.
        column: String,
    },
    /// A non-nullable column nothing writes on create, for a resource that serves a create page:
    /// not the form, not a Toasty `#[default(..)]`, not the tenant stamp, not a `create_column`.
    ///
    /// Add the field to the form, a default to the model, or a `create_column`; or make the
    /// resource `edit_only`.
    UnwrittenColumn {
        /// The column.
        column: String,
    },
    /// [`importable`](crate::ResourceDef::importable) on a resource that serves no create page:
    /// it has no record form, or is [`edit_only`](crate::ResourceDef::edit_only).
    ///
    /// Drop `importable`, or serve the create page.
    ImportWithoutCreate,
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
