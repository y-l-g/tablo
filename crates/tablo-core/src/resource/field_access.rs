//! [`FieldAccess`]: who may view, and who may change, one field of a resource's records; the
//! [`FieldMask`] a request reads them through; and the mount checks that keep every page under it.

use std::sync::Arc;

use topcoat::context::Cx;

use super::{Mounted, Resource, is_restricted_model};
use crate::{
    DeclarationError, DeclarationErrorKind, FieldAccessFault, Site, form::FormField,
    schema::FormMask, toasty_compat::model,
};

/// A request's rule over a field: whether it may.
type Rule = Arc<dyn Fn(&Cx) -> bool + Send + Sync>;

/// Who may view one field of a resource's records, and who may change it, which
/// [`ResourceDef::field_access`](crate::ResourceDef::field_access) declares.
///
/// A request the [`view`](Self::view) rule refuses sees no column, filter, grouping or detail
/// entry reading the field, no export column, and no control for it: an edit keeps the stored
/// value, and a create takes the control's default, else the blank answer. One the
/// [`edit`](Self::edit) rule refuses sees the control disabled. Either answers 400 to a
/// submission posting the field.
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

/// What is wrong with `declared`'s field access: a lens naming no field, two declarations over
/// one, a record title or a form watch reading a restricted field, and a column or filter that
/// does not say what it reads.
pub(crate) fn check<R: Resource>(declared: &Mounted<R>, errors: &mut Vec<DeclarationError>) {
    let rules = &declared.field_rules;
    let mut seen = Vec::new();
    for rule in rules {
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
        // A rule only over edits leaves the field shown to every user: a title or a watch over
        // it tells no one what they may not see.
        if rule.access.view.is_none() {
            continue;
        }
        let covered = |name: &str| rule.covers.iter().any(|covered| covered == name);
        if declared.record_title_field().is_some_and(covered) {
            errors.push(DeclarationError::of::<R>(
                site(field),
                fault(FieldAccessFault::RecordTitle),
            ));
        }
        let keys = FieldMask {
            hidden: rule.covers.clone(),
            locked: Vec::new(),
        }
        .form(&declared.fields);
        // The field's own controls may watch one another, as an embedded enum's watch its
        // variant: only a watch driving a control the user still sees tells what it holds.
        let tells = declared
            .form
            .watchers()
            .into_iter()
            .any(|(watched, driven)| {
                keys.hides(&watched) && driven.is_none_or(|driven| !keys.hides(&driven))
            });
        if tells {
            errors.push(DeclarationError::of::<R>(
                site(field),
                fault(FieldAccessFault::Watched),
            ));
        }
    }
    if rules.is_empty() {
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
    for filter in declared.table.declared_filters() {
        if filter.reads().is_none() {
            faults.push((Site::Table, undeclared("filter", filter.name())));
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

/// What shows the records of a model a mounted resource restricts a field of under no resource's
/// field access: a column whose includes load one and that masks none of what it shows.
pub(crate) fn check_related<R: Resource>(
    cx: &Cx,
    declared: &Mounted<R>,
    errors: &mut Vec<DeclarationError>,
) {
    let columns = declared
        .table
        .declared_columns()
        .iter()
        .map(|column| (Site::Table, column))
        .chain(
            declared
                .detail
                .columns()
                .into_iter()
                .map(|column| (Site::Detail, column)),
        );
    for (site, column) in columns {
        if column.masks_related() {
            continue;
        }
        let includes = column.includes().into_vec();
        let restricted = model::included_models::<R::Model>(&includes)
            .into_iter()
            .find(|(model, _)| is_restricted_model(cx, *model));
        if let Some((_, model)) = restricted {
            let fault = FieldAccessFault::UnmaskedRelation {
                column: column.name().to_string(),
                model,
            };
            errors.push(DeclarationError::of::<R>(
                site,
                DeclarationErrorKind::FieldAccess { fault },
            ));
        }
    }
}
