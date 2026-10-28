//! The check that a resource's form edits each editable column's field with the column's control
//! (ADR-0036).

use std::collections::HashSet;

use crate::{
    DeclarationError, DeclarationErrorKind, EditableColumnFault, Site,
    column::Editor,
    form::RecordForm,
    resource::{Mounted, Resource},
    schema::fields::Field,
};

/// Refuses each column of the resource's table editing its field in a cell that the edit route
/// would not save as the column posts it.
pub(super) fn check<R: Resource>(declared: &Mounted<R>, errors: &mut Vec<DeclarationError>) {
    let form = &declared.form;
    let guarded: HashSet<String> = form
        .condition_watchers()
        .into_iter()
        .filter_map(|(_, guarded)| guarded)
        .collect();
    for column in declared.table.declared_columns() {
        let Some(editor) = column.edits() else {
            continue;
        };
        let name = column.name();
        let field = form.fields().find(|field| field.name() == name);
        let fault = match field {
            _ if !<R::Form as RecordForm>::HAS_FORM => EditableColumnFault::NoForm,
            None => EditableColumnFault::Unplaced,
            Some(field) if !edits_with(field, editor) => {
                EditableColumnFault::OtherControl(editor.control())
            }
            Some(_) if guarded.contains(name) => EditableColumnFault::Conditional,
            Some(field) if field.is_disabled(true) => EditableColumnFault::Disabled,
            Some(_) => continue,
        };
        errors.push(DeclarationError::of::<R>(
            Site::Table,
            DeclarationErrorKind::EditableColumn {
                column: name.to_string(),
                fault,
            },
        ));
    }
}

/// Whether `field`'s control posts what `editor` posts.
fn edits_with(field: &Field, editor: Editor) -> bool {
    match editor {
        Editor::Toggle => field.is_checkbox(),
        Editor::Select => field
            .as_choice()
            .is_some_and(|choice| !choice.is_relationship() && !choice.is_multiple()),
        Editor::Text => field.as_text().is_some_and(|text| text.is_one_line()),
    }
}
