//! The checks a panel runs on each resource it mounts, against the app's models and the panel's
//! other resources, before it serves any: tenancy, the form against its record form, many-to-many
//! fields, the columns a create writes, and the actions' inputs.

use topcoat::context::Cx;

use crate::{
    ActionInputFault, DeclarationError, DeclarationErrorKind, ManyToManyFault, ModelColumns, Site,
    TenancyFault,
    form::RecordForm,
    resource::{InputSpec, Mounted, Resource, is_tenant_model, require_mounted},
    toasty_compat::{
        join::JoinTable,
        model::{self, AppSchema},
    },
};

/// A resource's declaration check: monomorphized once per registered resource, run by
/// [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel) with the app's values, the
/// panel's mounts and no request.
pub(crate) type ResourceCheck = fn(&Cx, &mut Vec<DeclarationError>);

/// Checks what a mounted resource promises before the panel serves it.
pub(crate) fn check_resource<R: Resource>(cx: &Cx, errors: &mut Vec<DeclarationError>) {
    let Ok(declared) = require_mounted::<R>(cx) else {
        return;
    };
    let tenancy = &declared.tenancy;
    if tenancy.is_undeclared() {
        check_undeclared_tenancy::<R>(cx, errors);
    }
    if let Some(Err(kind)) = tenancy.column_field() {
        errors.push(DeclarationError::of::<R>(Site::Tenancy, kind));
    }
    if tenancy.via_is_single() == Some(true) {
        errors.push(tenancy_error::<R>(TenancyFault::ViaOwnColumn));
    }
    let mut form_errors = declared.form.declaration_errors();
    form_errors.extend(declared.form.empty_choices());
    let form_is_sound = form_errors.is_empty();
    for (site, kinds) in [
        (Site::Table, declared.table.declaration_errors()),
        (Site::Form, form_errors),
        (Site::Detail, declared.detail.declaration_errors()),
    ] {
        errors.extend(
            kinds
                .into_iter()
                .map(|kind| DeclarationError::of::<R>(site.clone(), kind)),
        );
    }
    for (site, kinds) in [
        (Site::Table, declared.table.unavailable_sources(cx)),
        (Site::Detail, declared.detail.unavailable_sources(cx)),
    ] {
        errors.extend(
            kinds
                .into_iter()
                .map(|kind| DeclarationError::of::<R>(site.clone(), kind)),
        );
    }
    check_actions(cx, &declared, errors);
    check_form_declaration(cx, &declared, form_is_sound, errors);
}

/// A resource that declares no tenancy serves every tenant's rows, so its model must hold none:
/// it has no tenant column, and belongs to no model a mounted resource scopes by tenant.
fn check_undeclared_tenancy<R: Resource>(cx: &Cx, errors: &mut Vec<DeclarationError>) {
    let columns = <R::Model as ModelColumns>::TENANT_COLUMNS;
    if !columns.is_empty() {
        errors.push(tenancy_error::<R>(TenancyFault::Undeclared {
            model: model_name::<R>(),
            columns: columns.to_vec(),
        }));
    }
    for parent in model::belongs_tos::<R::Model>() {
        if is_tenant_model(cx, parent.target) {
            errors.push(tenancy_error::<R>(TenancyFault::UndeclaredChild {
                model: model_name::<R>(),
                relation: parent.name,
            }));
        }
    }
}

/// `R`'s model's name, as a lens spells it.
fn model_name<R: Resource>() -> &'static str {
    let name = std::any::type_name::<R::Model>();
    name.rsplit("::").next().unwrap_or(name)
}

/// A mistake in `R`'s tenancy declaration.
fn tenancy_error<R: Resource>(fault: TenancyFault) -> DeclarationError {
    DeclarationError::of::<R>(Site::Tenancy, DeclarationErrorKind::Tenancy { fault })
}

/// Every custom and header action's name is distinct among the resource's actions: the routes
/// dispatch by it. [`check_inputs`] checks their inputs.
/// [`ResourceDef::action`](crate::ResourceDef::action) checks each name is a route segment as
/// it compiles.
fn check_actions<R: Resource>(cx: &Cx, declared: &Mounted<R>, errors: &mut Vec<DeclarationError>) {
    let actions = declared
        .actions
        .entries()
        .iter()
        .map(|action| (action.name, action.input))
        .chain(
            declared
                .header_actions
                .entries()
                .iter()
                .map(|action| (action.name, action.input)),
        );
    check_inputs::<R>(cx, actions, errors);
}

/// Each of `T`'s actions has a name no other shares, and an input that declares no misdeclared
/// field, none the action's POST carries itself and no file field; an input with no field parses
/// an empty submission.
pub(crate) fn check_inputs<T: 'static>(
    cx: &Cx,
    actions: impl Iterator<Item = (&'static str, InputSpec)>,
    errors: &mut Vec<DeclarationError>,
) {
    let mut seen = std::collections::HashSet::new();
    for (name, spec) in actions {
        if !seen.insert(name) {
            errors.push(DeclarationError::of::<T>(
                Site::Registration,
                DeclarationErrorKind::DuplicateAction { name },
            ));
        }
        let input = (spec.schema)();
        let mut kinds = input.declaration_errors();
        kinds.extend(input.empty_choices());
        let mut faults: Vec<ActionInputFault> = input
            .fields()
            .filter_map(|field| {
                let name = field.name().to_string();
                if crate::resource::RESERVED_KEYS.contains(&name.as_str()) {
                    Some(ActionInputFault::ReservedField(name))
                } else if field.is_file() {
                    Some(ActionInputFault::FileField(name))
                } else if field.parent_key().is_some() {
                    Some(ActionInputFault::DependentChoice(name))
                } else {
                    None
                }
            })
            .collect();
        if !spec.takes_input && (spec.parse)(cx, &crate::form::FormValues::new()).is_err() {
            faults.push(ActionInputFault::RefusesEmpty);
        }
        kinds.extend(
            faults
                .into_iter()
                .map(|fault| DeclarationErrorKind::ActionInput {
                    action: name,
                    fault,
                }),
        );
        errors.extend(
            kinds
                .into_iter()
                .map(|kind| DeclarationError::of::<T>(Site::Registration, kind)),
        );
    }
}

/// A mistake in `R`'s form.
fn form_error<R: Resource>(kind: DeclarationErrorKind) -> DeclarationError {
    DeclarationError::of::<R>(Site::Form, kind)
}

/// Checks a resource's form declaration against its record form.
fn check_form_declaration<R: Resource>(
    cx: &Cx,
    declared: &Mounted<R>,
    form_is_sound: bool,
    errors: &mut Vec<DeclarationError>,
) {
    // A misdeclared form, such as one placing a control twice, would only echo its mistakes.
    if !form_is_sound {
        return;
    }
    if <R::Form as RecordForm>::HAS_FORM {
        check_form_inner(cx, declared, errors);
    }
}

fn check_form_inner<R: Resource>(
    cx: &Cx,
    declared: &Mounted<R>,
    errors: &mut Vec<DeclarationError>,
) {
    let (fields, form) = (declared.fields.as_slice(), &*declared.form);
    // The mount renders every field the form does not place, through `RecordForm::control`.
    for field in fields {
        for key in &field.keys {
            if !form.fields().any(|control| control.name() == key) {
                errors.push(form_error::<R>(DeclarationErrorKind::MissingControl {
                    field: field.name.to_string(),
                    key: key.clone(),
                }));
            }
        }
    }
    // The framework stamps the tenant column on create.
    if let Some(column) = tenant_column(declared)
        && let Some(field) = fields.iter().find(|field| field.keys.contains(&column))
    {
        errors.push(form_error::<R>(
            DeclarationErrorKind::FormClaimsTenantColumn {
                field: field.name.to_string(),
                column,
            },
        ));
    }
    for field in form.fields().filter(|field| {
        field
            .as_choice()
            .is_some_and(|choice| choice.has_composite_source())
    }) {
        errors.push(form_error::<R>(DeclarationErrorKind::CompositeKeyChoice {
            field: field.name().to_string(),
        }));
    }
    for field in form.fields() {
        if let Some(source) = field
            .as_choice()
            .and_then(|choice| choice.unavailable_source(cx))
        {
            errors.push(form_error::<R>(
                DeclarationErrorKind::UnregisteredOptionSource {
                    field: field.name().to_string(),
                    source,
                },
            ));
        }
    }
    if declared.tenancy.via_is_single() == Some(false) {
        check_via_foreign_keys(cx, declared, errors);
    }
    check_many_to_many(cx, declared, errors);
    // Whether the page exists, not whether the policy allows it: the checks run with no request,
    // so a policy reading the user would deny every create here and skip the check.
    if declared.creates {
        check_create_columns(declared, errors);
    }
    let columns = model::fields::<R::Model>();
    for field in form.fields().filter(|field| field.is_unique()) {
        let name = field.name();
        let backed = columns
            .iter()
            .any(|column| column.name == name && column.unique);
        if !backed {
            errors.push(form_error::<R>(DeclarationErrorKind::UniqueWithoutIndex {
                field: name.to_string(),
            }));
        }
        if !field.is_required() && !field.is_nullable() {
            errors.push(form_error::<R>(DeclarationErrorKind::OptionalUnique {
                field: name.to_string(),
            }));
        }
    }
}

/// Each many-to-many field of `R`'s form is a multiple choice over the records its join model
/// links, through a join model a link can write; and each multiple choice is such a field, since
/// no column stores a list of keys.
fn check_many_to_many<R: Resource>(
    cx: &Cx,
    declared: &Mounted<R>,
    errors: &mut Vec<DeclarationError>,
) {
    let Some(schema) = AppSchema::of(cx) else {
        return;
    };
    for field in declared.form.fields() {
        let name = field.name();
        let choice = field
            .as_choice()
            .filter(|choice| choice.is_multiple() && choice.is_relationship());
        let fault = if model::is_via::<R::Model>(name) {
            match (choice, JoinTable::of::<R::Model>(&schema, name)) {
                (_, Err(fault)) => Some(fault),
                (None, Ok(_)) => Some(ManyToManyFault::NotMultipleChoice),
                (Some(choice), Ok(join)) => (choice.source_model() != Some(join.target_model()))
                    .then_some(ManyToManyFault::OtherSource),
            }
        } else {
            field
                .as_choice()
                .is_some_and(|choice| choice.is_multiple())
                .then_some(ManyToManyFault::NoJoin)
        };
        if let Some(fault) = fault {
            errors.push(form_error::<R>(DeclarationErrorKind::ManyToMany {
                field: name.to_string(),
                fault,
            }));
        }
    }
}

/// Names `R`'s own tenant column.
fn tenant_column<R: Resource>(declared: &Mounted<R>) -> Option<String> {
    declared
        .tenancy
        .column_field()
        .and_then(Result::ok)
        .map(|field| field.name.clone())
}

/// Checks that every non-nullable column a create needs has a writer.
fn check_create_columns<R: Resource>(declared: &Mounted<R>, errors: &mut Vec<DeclarationError>) {
    let fields = &declared.fields;
    let mut create_columns = Vec::new();
    for column in &declared.create_columns {
        match column {
            Ok(name) => create_columns.push(name.as_str()),
            Err(kind) => errors.push(DeclarationError::of::<R>(Site::Registration, kind.clone())),
        }
    }
    let prefilled = model::prefilled_fields::<R::Model>();
    let tenant = tenant_column(declared);
    if let Some(column) = &tenant
        && create_columns.contains(&column.as_str())
    {
        errors.push(DeclarationError::of::<R>(
            Site::Registration,
            DeclarationErrorKind::CreateColumnsNameTenant {
                column: column.clone(),
            },
        ));
    }
    for field in &model::fields::<R::Model>() {
        let name = field.name.as_str();
        let filled = field.nullable
            || field.relation
            || prefilled.get(field.index).copied().unwrap_or(false)
            || tenant.as_deref() == Some(name)
            || fields.iter().any(|claim| claim.name == name)
            || create_columns.contains(&name);
        if !filled {
            errors.push(form_error::<R>(DeclarationErrorKind::UnwrittenColumn {
                column: name.to_string(),
            }));
        }
    }
}

/// A `Tenancy::via` resource inherits its tenant from the parent its foreign key names, so the
/// form must write that key through a relationship field over the parent's tenant-scoped
/// resource: the write re-checks only such a field's key against the request's tenant.
fn check_via_foreign_keys<R: Resource>(
    cx: &Cx,
    declared: &Mounted<R>,
    errors: &mut Vec<DeclarationError>,
) {
    let form = &declared.form;
    // The `belongs_to` relation the `via` lens steps through first.
    let Some(model::BelongsTo {
        name: relation,
        target: parent,
        foreign_keys: keys,
    }) = declared
        .tenancy
        .via_hop()
        .and_then(model::belongs_to::<R::Model>)
    else {
        errors.push(tenancy_error::<R>(TenancyFault::ViaWithoutBelongsTo));
        return;
    };
    let unguarded: Vec<String> = keys
        .into_iter()
        .filter(|key| {
            !form.fields().any(|field| {
                field.name() == key
                    && field
                        .as_choice()
                        .and_then(|choice| choice.tenant_scoped_model(cx))
                        == Some(parent)
            })
        })
        .collect();
    if !unguarded.is_empty() {
        errors.push(form_error::<R>(DeclarationErrorKind::Tenancy {
            fault: TenancyFault::UnguardedForeignKey {
                relation,
                keys: unguarded,
            },
        }));
    }
}
