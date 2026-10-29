//! [`FieldAccess`]: who may view, and who may change, one field of a resource's records; the
//! [`FieldMask`] a request reads them through; and the mount checks that keep every page under it.

use std::sync::Arc;

use topcoat::context::Cx;

use super::{Mounted, Resource, is_mounted, is_restricted_model, model_rules, restricts_any};
use crate::{
    DeclarationError, DeclarationErrorKind, FieldAccessFault, Site,
    column::{BoxColumn, Related},
    form::FormField,
    schema::{FormMask, Schema},
    toasty_compat::{model, pk},
};

/// A request's rule over a field: whether it may.
type Rule = Arc<dyn Fn(&Cx) -> bool + Send + Sync>;

/// Who may view one field of a model's records, and who may change it, which
/// [`ResourceDef::field_access`](crate::ResourceDef::field_access) declares: every resource the
/// panel mounts over the model obeys the rule.
///
/// A request the [`view`](Self::view) rule refuses sees no column, filter, grouping, detail entry
/// or relation column reading the field, no export column, and no control for it: an edit keeps
/// the stored value, and a create takes the control's default, else the blank answer. When the
/// field has no default and the record form requires it, or its rules refuse a blank answer, as
/// an `accepted` checkbox does, that request may not create. One the
/// [`edit`](Self::edit) rule refuses sees the control disabled. Either answers 400 to a submission
/// posting the field.
///
/// ```rust
/// # use topcoat::context::Cx;
/// # fn is_hr(_cx: &Cx) -> bool { true }
/// # fn is_payroll(_cx: &Cx) -> bool { true }
/// use tablo_core::FieldAccess;
///
/// FieldAccess::new().view(is_hr).edit(is_payroll);
/// ```
#[derive(Clone, Default)]
pub struct FieldAccess {
    view: Option<Rule>,
    edit: Option<Rule>,
}

impl FieldAccess {
    /// Every request views and changes the field, until [`view`](Self::view) or
    /// [`edit`](Self::edit) narrows it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Let only the requests `rule` allows view the field.
    #[must_use]
    pub fn view(mut self, rule: impl Fn(&Cx) -> bool + Send + Sync + 'static) -> Self {
        self.view = Some(Arc::new(rule));
        self
    }

    /// Let only the requests `rule` allows change the field, among the ones that view it.
    #[must_use]
    pub fn edit(mut self, rule: impl Fn(&Cx) -> bool + Send + Sync + 'static) -> Self {
        self.edit = Some(Arc::new(rule));
        self
    }

    /// Whether `cx` views the field.
    fn views(&self, cx: &Cx) -> bool {
        self.view.as_ref().is_none_or(|rule| rule(cx))
    }

    /// Whether `cx` changes the field.
    fn edits(&self, cx: &Cx) -> bool {
        self.views(cx) && self.edit.as_ref().is_none_or(|rule| rule(cx))
    }
}

impl std::fmt::Debug for FieldAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldAccess")
            .field("view", &self.view.is_some())
            .field("edit", &self.edit.is_some())
            .finish()
    }
}

/// One declared [`FieldAccess`], resolved against the model.
#[derive(Debug, Clone)]
pub(crate) struct FieldRule {
    /// The field, or why the lens names none; a placeholder spelling the steps then.
    field: Result<String, (String, DeclarationErrorKind)>,
    /// The field and the fields a `belongs_to` pairs it with: its relation, or its foreign keys.
    covers: Vec<String>,
    /// Whether the field is the inverse side of a relation, which no mask reaches.
    inverse: bool,
    access: FieldAccess,
}

impl FieldRule {
    /// `access` over the root field `path` names.
    pub(crate) fn new<M, T>(path: &toasty::stmt::Path<M, T>, access: FieldAccess) -> Self
    where
        M: toasty::schema::Model,
    {
        let path = model::ModelPath::of(path);
        let field =
            model::field::<M>(&path).map_err(|error| (format!("{:?}", path.steps()), error));
        let covers = match &field {
            Ok(field) => paired::<M>(field),
            Err(_) => Vec::new(),
        };
        let inverse = field
            .as_ref()
            .is_ok_and(|field| model::is_inverse::<M>(&field.name));
        Self {
            field: field.map(|field| field.name),
            covers,
            inverse,
            access,
        }
    }

    /// Whether the rule restricts who views the field, not only who changes it.
    pub(crate) fn restricts_view(&self) -> bool {
        self.access.view.is_some()
    }
}

/// `field` and the fields a `belongs_to` pairs it with: a relation's foreign keys, or the
/// relations whose foreign key it is. Hiding a key hides the relation that shows its record.
fn paired<M: toasty::schema::Model>(field: &model::FieldInfo) -> Vec<String> {
    let mut covers = vec![field.name.clone()];
    for relation in model::belongs_tos::<M>() {
        let pairs = relation.name == field.name || relation.foreign_keys.contains(&field.name);
        if pairs {
            covers.push(relation.name);
            covers.extend(relation.foreign_keys);
        }
    }
    covers.sort();
    covers.dedup();
    covers
}

/// The root fields one request may not view, and the ones it may view but not change.
#[derive(Debug, Clone, Default)]
pub(crate) struct FieldMask {
    hidden: Vec<String>,
    locked: Vec<String>,
}

impl FieldMask {
    /// What `rules` mask for `cx`.
    pub(crate) fn of(cx: &Cx, rules: &[FieldRule]) -> Self {
        let mut mask = Self::default();
        for rule in rules {
            if !rule.access.views(cx) {
                mask.hidden.extend(rule.covers.iter().cloned());
            } else if !rule.access.edits(cx) {
                mask.locked.extend(rule.covers.iter().cloned());
            }
        }
        mask
    }

    /// The fields the request may not view.
    pub(crate) fn hidden(&self) -> &[String] {
        &self.hidden
    }

    /// The keys of `fields` it hides, and the ones it locks.
    pub(crate) fn form<K>(&self, fields: &[FormField<K>]) -> FormMask {
        let keys = |names: &[String]| -> Vec<String> {
            fields
                .iter()
                .filter(|field| names.iter().any(|name| name == field.name))
                .flat_map(|field| field.keys.iter().cloned())
                .collect()
        };
        FormMask::new(keys(&self.hidden), keys(&self.locked))
    }
}

/// What is wrong with `declared`'s field access: a lens naming no field, two declarations over one
/// field, a rule over a primary key, a record title or a form watch reading a field a mounted
/// resource over the model restricts, and a column or filter that does not say what it reads.
pub(crate) fn check<R: Resource>(
    cx: &Cx,
    declared: &Mounted<R>,
    errors: &mut Vec<DeclarationError>,
) {
    let model = model_rules(
        cx,
        <R::Model as toasty::schema::Model>::id(),
        &declared.field_rules,
    );
    let key_fields = pk::key_names::<R::Model>();
    let mut seen = Vec::new();
    for rule in &declared.field_rules {
        let site = |field: &str| Site::FieldAccess(field.to_string());
        let fault = |fault| DeclarationErrorKind::FieldAccess { fault };
        let field = match &rule.field {
            Ok(field) => field,
            Err((placeholder, error)) => {
                errors.push(DeclarationError::of::<R>(site(placeholder), error.clone()));
                continue;
            }
        };
        if seen.contains(&field) {
            errors.push(DeclarationError::of::<R>(
                site(field),
                fault(FieldAccessFault::Duplicate),
            ));
        }
        seen.push(field);
        if rule.inverse {
            errors.push(DeclarationError::of::<R>(
                site(field),
                fault(FieldAccessFault::Inverse),
            ));
            continue;
        }
        let trash = declared.soft_delete.as_ref();
        if trash.is_some_and(|trash| trash.field().is_ok_and(|(_, name)| name == field)) {
            errors.push(DeclarationError::of::<R>(
                site(field),
                fault(FieldAccessFault::SoftDelete),
            ));
        }
        if rule.restricts_view() && key_fields.contains(field) {
            errors.push(DeclarationError::of::<R>(
                site(field),
                fault(FieldAccessFault::RecordKey),
            ));
        }
    }
    // A rule only over edits leaves the field shown to every user: a title or a watch over it
    // tells no one what they may not see. The restrictions are the model's, every mounted
    // resource's, since a title or a watch reads the record whichever resource shows it.
    let hiding = model.iter().filter(|rule| rule.restricts_view());
    let title_hidden = declared.record_title_field().is_some_and(|title| {
        hiding
            .clone()
            .any(|rule| rule.covers.iter().any(|covered| covered == title))
    });
    if title_hidden {
        errors.push(DeclarationError::of::<R>(
            Site::RecordTitle,
            DeclarationErrorKind::FieldAccess {
                fault: FieldAccessFault::RecordTitle,
            },
        ));
    }
    let watchers = declared.form.watchers();
    // The field's own controls may watch one another, as an embedded enum's watch its variant:
    // only a watch driving a control the user still sees tells what it holds.
    let watched = hiding.clone().any(|rule| {
        let keys = FieldMask {
            hidden: rule.covers.clone(),
            locked: Vec::new(),
        }
        .form(&declared.fields);
        watchers.iter().any(|(watched, driven)| {
            keys.hides(watched) && driven.as_ref().is_none_or(|driven| !keys.hides(driven))
        })
    });
    if watched {
        errors.push(DeclarationError::of::<R>(
            Site::Form,
            DeclarationErrorKind::FieldAccess {
                fault: FieldAccessFault::Watched,
            },
        ));
    }
    if model.is_empty() {
        return;
    }
    let undeclared = |what, name: &str| DeclarationErrorKind::FieldAccess {
        fault: FieldAccessFault::UndeclaredReads {
            what,
            name: name.to_string(),
        },
    };
    let mut faults = Vec::new();
    for column in declared.table.declared_columns() {
        if column.reads().is_none() {
            faults.push((Site::Table, undeclared("column", column.name())));
        }
    }
    if declared.table.row_color_reads_undeclared() {
        faults.push((Site::Table, undeclared("row color", "row_color")));
    }
    for filter in declared.table.declared_filters() {
        if filter.reads().is_none() {
            faults.push((Site::Table, undeclared("filter", filter.name())));
        }
    }
    for tab in declared.table.declared_tabs() {
        if tab.reads().is_none() {
            faults.push((Site::Table, undeclared("tab", tab.key())));
        }
    }
    for column in declared.detail.undeclared_reads() {
        faults.push((Site::Detail, undeclared("column", column)));
    }
    errors.extend(
        faults
            .into_iter()
            .map(|(site, kind)| DeclarationError::of::<R>(site, kind)),
    );
}

/// What `declared`'s table and detail page show of the records of a model a mounted resource
/// restricts a field of, under no field access a panel can mask.
pub(crate) fn check_columns<R: Resource>(
    cx: &Cx,
    declared: &Mounted<R>,
    errors: &mut Vec<DeclarationError>,
) {
    if !restricts_any(cx) {
        return;
    }
    let table: Vec<_> = declared.table.declared_columns().iter().collect();
    let row_color = declared.table.tints_rows().then_some("row color");
    check_page::<R>(cx, Site::Table, &table, row_color, errors);
    check_page::<R>(cx, Site::Detail, &declared.detail.columns(), None, errors);
}

/// What `columns`, the columns of one page, show under no field access a panel can mask. A column
/// reading related records no mounted resource masks is refused over a restricted model its own
/// includes load; a column reading its row, and `row_reader`, the page's other closure over its
/// rows, are refused on a page whose columns load one, since a closure may read the related record
/// through any of them.
fn check_page<R: Resource>(
    cx: &Cx,
    site: Site,
    columns: &[&BoxColumn<R::Model>],
    row_reader: Option<&str>,
    errors: &mut Vec<DeclarationError>,
) {
    let loads: Vec<Option<Vec<(model::ModelId, String)>>> = columns
        .iter()
        .map(|column| model::included_models::<R::Model>(&column.includes().into_vec()))
        .collect();
    let unresolved = loads.iter().any(Option::is_none);
    let page_restricted = loads
        .iter()
        .flatten()
        .flatten()
        .find(|(model, _)| is_restricted_model(cx, *model))
        .map(|(_, name)| name.clone());
    let reads_row = |name: String| match (unresolved, &page_restricted) {
        (true, _) => Some(FieldAccessFault::UnresolvedRelation { name }),
        (false, Some(model)) => Some(FieldAccessFault::UnmaskedRelation {
            name,
            model: model.clone(),
        }),
        (false, None) => None,
    };
    let push = |errors: &mut Vec<DeclarationError>, fault| {
        errors.push(DeclarationError::of::<R>(
            site.clone(),
            DeclarationErrorKind::FieldAccess { fault },
        ));
    };
    if let Some(fault) = row_reader.and_then(|name| reads_row(name.to_string())) {
        push(errors, fault);
    }
    for (column, own) in columns.iter().zip(&loads) {
        let name = column.name().to_string();
        let fault = match column.related() {
            Related::None => continue,
            Related::Source(source) if is_mounted(cx, source) => continue,
            Related::Row => match reads_row(name) {
                Some(fault) => fault,
                None => continue,
            },
            Related::Source(_) | Related::Unmasked => match own {
                None => FieldAccessFault::UnresolvedRelation { name },
                Some(models) => {
                    match models
                        .iter()
                        .find(|(model, _)| is_restricted_model(cx, *model))
                    {
                        Some((_, model)) => FieldAccessFault::UnmaskedRelation {
                            name,
                            model: model.clone(),
                        },
                        None => continue,
                    }
                }
            },
        };
        push(errors, fault);
    }
}

/// Whether a mounted resource over `model` restricts who views its `field`.
pub(crate) fn restricts_field(cx: &Cx, model: model::ModelId, field: &str) -> bool {
    model_rules(cx, model, &[])
        .iter()
        .any(|rule| rule.restricts_view() && rule.covers.iter().any(|covered| covered == field))
}

/// What `schema`'s relationship choices show that no mount may: records under a label a panel
/// cannot mask, and a dependent choice narrowed by a field a mounted resource restricts.
pub(crate) fn check_choices<T: 'static, F>(
    cx: &Cx,
    schema: &Schema<F>,
    site: Site,
    errors: &mut Vec<DeclarationError>,
) {
    let mounted = |source| is_mounted(cx, source);
    let restricted = |model: model::ModelId| is_restricted_model(cx, model);
    for field in schema.fields() {
        let Some(choice) = field.as_choice() else {
            continue;
        };
        let name = field.name();
        if let Some(model) = choice.unmasked_model(mounted, restricted) {
            errors.push(DeclarationError::of::<T>(
                site.clone(),
                DeclarationErrorKind::FieldAccess {
                    fault: FieldAccessFault::UnmaskedRelation {
                        name: name.to_string(),
                        model: model.to_string(),
                    },
                },
            ));
        }
        if let Some(column) = choice.narrowed_by(|model, column| restricts_field(cx, model, column))
        {
            errors.push(DeclarationError::of::<T>(
                site.clone(),
                DeclarationErrorKind::FieldAccess {
                    fault: FieldAccessFault::NarrowedBy {
                        field: column.to_string(),
                    },
                },
            ));
        }
    }
}

/// What `declared`'s filters and tabs show or read that no mount may: records under a label a
/// panel cannot mask, and records a relation the filter's or the tab's reads reach holds, of a
/// model a mounted resource restricts a field of.
pub(crate) fn check_filters<R: Resource>(
    cx: &Cx,
    declared: &Mounted<R>,
    errors: &mut Vec<DeclarationError>,
) {
    if !restricts_any(cx) {
        return;
    }
    let mounted = |source| is_mounted(cx, source);
    let restricted = |model: model::ModelId| is_restricted_model(cx, model);
    for filter in declared.table.declared_filters() {
        let name = filter.name().to_string();
        let fault = if let Some(model) = filter.unmasked_model(&mounted, &restricted) {
            Some(FieldAccessFault::UnmaskedRelation {
                name,
                model: model.to_string(),
            })
        } else {
            relation_fault(cx, name, filter.reads())
        };
        errors.extend(fault.map(|fault| {
            DeclarationError::of::<R>(Site::Table, DeclarationErrorKind::FieldAccess { fault })
        }));
    }
    for tab in declared.table.declared_tabs() {
        let fault = relation_fault(cx, tab.key().to_string(), tab.reads());
        errors.extend(fault.map(|fault| {
            DeclarationError::of::<R>(Site::Table, DeclarationErrorKind::FieldAccess { fault })
        }));
    }
}

/// The relation a filter's or a tab's `reads`, named `name`, reach that no mount may read: one
/// whose model a mounted resource restricts a field of, or one the mount cannot resolve.
fn relation_fault<M: toasty::schema::Model>(
    cx: &Cx,
    name: String,
    reads: Option<crate::Reads<M>>,
) -> Option<FieldAccessFault> {
    let reads = reads?;
    match model::read_relations::<M>(reads.fields()) {
        Some(models) if !reads.is_unresolved() => models
            .into_iter()
            .find(|(model, _)| is_restricted_model(cx, *model))
            .map(|(_, model)| FieldAccessFault::UnmaskedRelation { name, model }),
        _ => Some(FieldAccessFault::UnresolvedRelation { name }),
    }
}
