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
    /// The resource or page itself: its slug, its actions, its `CREATE_COLUMNS`, or a second
    /// home page.
    Registration,
    /// `Resource::tenancy`.
    Tenancy,
    /// `Resource::table`.
    Table,
    /// `Resource::form`, checked against the record form.
    Form,
    /// `Resource::view`.
    View,
    /// The `Resource::relations` entry with this key.
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
    /// A path segment the panel routes cannot be a literal URL segment.
    InvalidSegment {
        /// What names the segment: `panel prefix`, `Resource::slug` or `Page::slug`.
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
    /// Two relations to the same resource.
    DuplicateRelation,
    /// A relation to a resource the panel does not register.
    UnregisteredRelation,
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
    /// Two schema fields share a name.
    DuplicateField {
        /// The shared name.
        name: String,
    },
    /// A `form()` override that declares no control over a record form with fields.
    EmptyFormOverride,
    /// A form schema on a resource whose `Form` serves no form.
    FormWithoutRecordForm,
    /// A resource whose policy allows create but that has no form.
    CreateWithoutForm,
    /// A form control no record-form field binds, so its input is never written.
    UnboundControl {
        /// The control's key.
        control: String,
    },
    /// A form control more than one record-form field binds.
    ControlBoundTwice {
        /// The control's key.
        control: String,
    },
    /// A record-form field binding a key the form declares no control for.
    MissingControl {
        /// The record-form field.
        field: String,
        /// The key it binds.
        key: String,
    },
    /// A control that may be posted empty over a record-form field with no blank answer.
    NoBlankAnswer {
        /// The control's key.
        control: String,
        /// The record-form field.
        field: String,
        /// Whether the control sits inside a `Repeater`; otherwise it is optional.
        in_repeater: bool,
    },
    /// A tenant-scoped resource's record form claims the tenant column.
    FormClaimsTenantColumn {
        /// The record-form field.
        field: String,
        /// The tenant column.
        column: String,
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
    /// `CREATE_COLUMNS` names the tenant column.
    CreateColumnsNameTenant {
        /// The tenant column.
        column: String,
    },
    /// `CREATE_COLUMNS` names a field the model does not have.
    UnknownCreateColumn {
        /// The name.
        column: &'static str,
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
            Self::InvalidSegment {
                item,
                segment,
                fault,
            } => write!(f, "{item} '{segment}' {fault}"),
            Self::PrefixOverlapsRuntime { prefix } => write!(
                f,
                "prefix '{prefix}' overlaps Topcoat's runtime endpoints at '{}'",
                crate::auth::RUNTIME_PREFIX
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
            Self::DuplicateRelation => {
                f.write_str("declared twice: each related resource is one relation")
            }
            Self::UnregisteredRelation => f.write_str(
                "the related resource is not registered on this panel: declare it with \
                 `Panel::resource`",
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
            Self::TenancyColumnNotAField => f.write_str(
                "the `Tenancy::column` lens names no field of the model: name a UUID field, or \
                 use `Tenancy::via` for a tenant reached through a relation",
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
                "no column: declare columns with `Table::new(columns)` or in `Resource::table`",
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
            Self::DuplicateField { name } => write!(
                f,
                "two fields are named '{name}': each input needs a distinct field"
            ),
            Self::EmptyFormOverride => f.write_str(
                "`form()` declares no control over a record form with fields: drop the override \
                 to render the derived schema",
            ),
            Self::FormWithoutRecordForm => f.write_str(
                "a form schema is declared, but `type Form` serves no form: name the record form \
                 there",
            ),
            Self::CreateWithoutForm => f.write_str(
                "the policy allows create, but there is no form: name the record form in `type \
                 Form`",
            ),
            Self::UnboundControl { control } => write!(
                f,
                "control `{control}` is bound by no record-form field, so what the user types \
                 there is never written"
            ),
            Self::ControlBoundTwice { control } => write!(
                f,
                "control `{control}` is bound by more than one record-form field"
            ),
            Self::MissingControl { field, key } => write!(
                f,
                "record-form field `{field}` binds key `{key}`, but no control declares it"
            ),
            Self::NoBlankAnswer {
                control,
                field,
                in_repeater,
            } => {
                let place = if *in_repeater {
                    "sits inside a `Repeater`, so it may be posted empty"
                } else {
                    "is optional"
                };
                write!(
                    f,
                    "control `{control}` {place}, but record-form field `{field}` has no blank \
                     answer: declare `#[form(blank = ..)]`, make the field an `Option`, or make \
                     the control required"
                )
            }
            Self::FormClaimsTenantColumn { field, column } => write!(
                f,
                "record-form field `{field}` claims the tenant column `{column}`, which the \
                 framework stamps on create: drop it from the form"
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
            Self::CreateColumnsNameTenant { column } => write!(
                f,
                "`CREATE_COLUMNS` names the tenant column `{column}`, which the framework stamps \
                 on create: drop it there and delegate to `write_create`"
            ),
            Self::UnknownCreateColumn { column } => write!(
                f,
                "`CREATE_COLUMNS` names `{column}`, which is no field of the model"
            ),
            Self::UnwrittenColumn { column } => write!(
                f,
                "the policy allows create, but nothing writes the non-nullable column `{column}`: \
                 the record form has no such field, toasty fills no `#[default(..)]` for it, and \
                 `CREATE_COLUMNS` does not name it, so every create would fail at the driver"
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
