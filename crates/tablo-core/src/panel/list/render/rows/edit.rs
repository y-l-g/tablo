//! An editable cell: its control in a form that posts the field alone to the record's edit route,
//! which completes every other key from the stored record (ADR-0036).

use topcoat::{context::Cx, runtime::Event, view::*};

use crate::form::VERSION_FIELD;

/// One editable cell's form.
pub(super) struct CellForm<'a> {
    /// Its DOM id, distinct per row and column.
    pub(super) id: String,
    /// The record's edit route, with the table's return target.
    pub(super) action: String,
    /// The column's name, which is the form key it posts.
    pub(super) column: String,
    /// The record's version, when the model keeps one.
    pub(super) version: Option<u64>,
    /// Whether the control saves when it changes; a text input saves on Enter instead.
    pub(super) saves_on_change: bool,
    /// The column's control.
    pub(super) control: BoxView<'a>,
}

/// `form` as the cell renders it.
pub(super) fn cell_form<'a>(cx: &'a Cx, form: CellForm<'a>) -> BoxView<'a> {
    let csrf = crate::csrf::current_token(cx);
    let version = form.version.map(|version| version.to_string());
    let id = form.id.clone();
    let on_change = form.saves_on_change.then_some("");
    view! {
        cx =>
        <form
            id=(form.id)
            method="post"
            action=(form.action)
            data-cell-edit=(form.column)
            data-save-on-change=(on_change)
            @change=$(|_e: Event| {
                raw!(
                    "((f) => f && 'saveOnChange' in f.dataset && f.requestSubmit())(document.getElementById(String(${id})))",
                    (),
                );
            })
        >
            (crate::csrf::field(cx, &csrf))
            if let Some(version) = version {
                <input type="hidden" name=(VERSION_FIELD) value=(version)>
            }
            (form.control)
        </form>
    }
    .boxed()
}
