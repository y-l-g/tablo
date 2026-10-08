//! The declaration mistakes a panel refuses to mount with, and the one place their wording lives.

use std::fmt;

/// Why [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel) refused a panel:
/// every declaration mistake it found.
///
/// The router's builder returns it as a [`topcoat::Error`]; recover it with
/// `error.downcast_ref::<MountError>()`.
#[derive(Debug)]
pub struct MountError {
    panel: String,
    errors: Vec<DeclarationError>,
}

impl MountError {
    pub(crate) fn new(panel: &str, errors: Vec<DeclarationError>) -> Self {
        Self {
            panel: panel.to_string(),
            errors,
        }
    }

    /// The refused panel's prefix.
    pub fn panel(&self) -> &str {
        &self.panel
    }

    /// Every mistake found, in declaration order.
    pub fn errors(&self) -> &[DeclarationError] {
        &self.errors
    }
}

impl fmt::Display for MountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "panel '{}' cannot mount:", self.panel)?;
        for error in &self.errors {
            write!(f, "\n  - {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for MountError {}

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
                Site::View => f.write_str(" view")?,
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
    /// `ResourceDef::view`.
    View,
    /// The `ResourceDef::relation` to the resource with this slug, or this type when the panel
    /// does not register it.
    Relation(String),
}

/// Why an action's input cannot be served:
/// [`DeclarationErrorKind::ActionInput`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ActionInputFault {
    /// A field named like a key the action's POST carries itself: `csrf_token`, `confirm`, `ids`
    /// or `-input`.
    ReservedField(String),
    /// A file field, whose upload an action's POST does not read.
    FileField(String),
    /// No field, but a parse that refuses an empty submission, so the action could never run.
    RefusesEmpty,
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
    /// A form declares a file field, but the panel installs no `Uploader` to store its bytes.
    FileFieldWithoutUploader {
        /// The file field's name.
        field: String,
    },
    /// A path segment the panel routes cannot be a literal URL segment.
    InvalidSegment {
        /// What names the segment: `panel prefix`, `ResourceDef::slug` or `Page::slug`.
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
    /// Two relations to the same resource.
    DuplicateRelation,
    /// A relation to a resource the panel does not register.
    UnregisteredRelation,
    /// A relationship field whose options come from a resource the panel does not register.
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
    /// An embedded path or value its declaration never bound to the app schema.
    Unbound {
        /// The path's model, or the embedded value's type.
        item: &'static str,
    },
    /// A `Tenancy::column` lens that names no field of the model.
    TenancyColumnNotAField,
    /// A `Tenancy::via` lens that names one field of the model.
    TenancyViaOwnColumn,
    /// A `Tenancy::via` lens whose first step is no `belongs_to` relation.
    TenancyViaWithoutBelongsTo,
    /// A `Tenancy::via` resource whose form writes the parent's foreign key other than through a
    /// relationship field over a tenant-scoped resource of the parent's model.
    UnguardedForeignKey {
        /// The `belongs_to` relation the lens steps through.
        relation: String,
        /// The foreign-key columns written another way.
        keys: Vec<String>,
    },
    /// A table with no column.
    NoColumns,
    /// `Table::paginate(0)`.
    ZeroPageSize,
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
    /// A field a condition hides has no blank answer, so a submission hiding it cannot parse.
    RequiredConditionalField {
        /// The field.
        field: String,
    },
    /// A resource whose policy allows create but that has no form.
    CreateWithoutForm,
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
    /// A relationship field over a model with a composite primary key.
    CompositeKeyChoice {
        /// The field.
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
    /// A non-nullable column nothing writes on create.
    UnwrittenColumn {
        /// The column.
        column: String,
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
                "auth is on, but the Db does not register its shipped models ({}): register them \
                 with `toasty::models!(…, tablo_core::auth::AdminUser, \
                 tablo_core::auth::AuthSession)`, or opt out with `.auth(Auth::disabled())`",
                models.join(", ")
            ),
            Self::ShellAssetsWithoutBundle => f.write_str(
                "shell_assets need the router's asset bundle: install it with `.assets(..)` \
                 before mounting the panel",
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
            Self::ActionInput { action, fault } => match fault {
                ActionInputFault::ReservedField(field) => write!(
                    f,
                    "action '{action}' declares an input field named '{field}', which its POST \
                     carries itself: rename the field"
                ),
                ActionInputFault::FileField(field) => write!(
                    f,
                    "action '{action}' declares a file field '{field}' in its input, which an \
                     action's POST does not upload: take the file in a record form"
                ),
                ActionInputFault::RefusesEmpty => write!(
                    f,
                    "action '{action}' declares no input field, but its input refuses an empty \
                     submission: declare the fields it parses, or name `()`"
                ),
            },
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
            Self::Unbound { item } => write!(
                f,
                "an embedded path into `{item}` is not bound to the app schema: a panel binds the \
                 declarations it mounts; bind one built outside a panel with `.bind(&db)`"
            ),
            Self::TenancyColumnNotAField => f.write_str(
                "the `Tenancy::column` lens names no field of the model: name a field typed \
                 `TenantId`, or use `Tenancy::via` for a tenant reached through a relation",
            ),
            Self::TenancyViaOwnColumn => f.write_str(
                "the `Tenancy::via` lens names one field of the model: use `Tenancy::column` for \
                 the model's own tenant column",
            ),
            Self::TenancyViaWithoutBelongsTo => f.write_str(
                "the `Tenancy::via` lens does not start at a `belongs_to` relation: the tenant \
                 must be the parent's its foreign key names",
            ),
            Self::UnguardedForeignKey { relation, keys } => write!(
                f,
                "foreign key `{}` is not written through a relationship field over a \
                 tenant-scoped resource of `{relation}`'s model: the write re-checks only such a \
                 field's key, so any other could attach the row to another tenant's parent",
                keys.join("`, `")
            ),
            Self::NoColumns => f.write_str(
                "no column: declare columns with `Table::new(columns)` or in `ResourceDef::table`",
            ),
            Self::ZeroPageSize => f.write_str("`Table::paginate` needs a page size of at least 1"),
            Self::DuplicateColumn { name } => write!(
                f,
                "two columns are named '{name}': each column needs a distinct name"
            ),
            Self::DuplicateFilter { name } => write!(
                f,
                "two filters are named '{name}': each filter needs a distinct name"
            ),
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
            Self::RequiredConditionalField { field } => write!(
                f,
                "field '{field}' is conditional but has no blank answer: a submission that hides \
                 it posts nothing, so make it an `Option` or give it `#[form(blank = ..)]`"
            ),
            Self::CreateWithoutForm => f.write_str(
                "the policy allows create, but there is no form: name the record form in `type \
                 Form`",
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
                "relationship field `{field}` loads a model with a composite primary key, which \
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
                "the policy allows create, but nothing writes the non-nullable column `{column}`: \
                 the record form has no such field, toasty fills no `#[default(..)]` for it, and \
                 no `create_column` names it, so every create would fail at the driver"
            ),
        }
    }
}

/// Why a path segment cannot be a literal URL segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SegmentFault {
    /// The segment is empty.
    Empty,
    /// The segment is `.` or `..`.
    Dot,
    /// The segment holds a character a route segment cannot.
    Char(char),
}

impl fmt::Display for SegmentFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("is empty"),
            Self::Dot => f.write_str("is '.' or '..'"),
            Self::Char(c) => write!(
                f,
                "contains {c:?}: quotes, backslashes, control characters, whitespace, URL \
                 punctuation and the route pattern characters '{{', '}}', '(' and ')' are refused"
            ),
        }
    }
}

/// Why `segment` cannot be a literal URL segment, if it cannot.
///
/// A `const fn`, so an action's `NAME` is checked when the panel's code compiles.
pub(crate) const fn segment_fault(segment: &str) -> Option<SegmentFault> {
    let bytes = segment.as_bytes();
    if bytes.is_empty() {
        return Some(SegmentFault::Empty);
    }
    if matches!(bytes, b"." | b"..") {
        return Some(SegmentFault::Dot);
    }
    let mut at = 0;
    while at < bytes.len() {
        let (c, width) = decode_char(bytes, at);
        if c.is_control()
            || c.is_whitespace()
            || matches!(
                c,
                '"' | '\\' | '/' | '?' | '#' | '%' | '&' | '=' | '{' | '}' | '(' | ')'
            )
        {
            return Some(SegmentFault::Char(c));
        }
        at += width;
    }
    None
}

/// The character starting at byte `at` of valid UTF-8, and its width.
const fn decode_char(bytes: &[u8], at: usize) -> (char, usize) {
    let lead = bytes[at] as u32;
    let (code, width) = if lead < 0x80 {
        (lead, 1)
    } else if lead < 0xE0 {
        ((lead & 0x1F) << 6 | tail(bytes, at + 1), 2)
    } else if lead < 0xF0 {
        (
            (lead & 0x0F) << 12 | tail(bytes, at + 1) << 6 | tail(bytes, at + 2),
            3,
        )
    } else {
        (
            (lead & 0x07) << 18
                | tail(bytes, at + 1) << 12
                | tail(bytes, at + 2) << 6
                | tail(bytes, at + 3),
            4,
        )
    };
    match char::from_u32(code) {
        Some(c) => (c, width),
        None => panic!("a str holds valid UTF-8"),
    }
}

/// The six payload bits of the continuation byte at `at`.
const fn tail(bytes: &[u8], at: usize) -> u32 {
    bytes[at] as u32 & 0x3F
}

#[cfg(test)]
mod tests;
