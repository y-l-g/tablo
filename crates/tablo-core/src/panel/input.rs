//! An action's input in a dialog over the page that offers it: a table's rows and bulk bar, or a
//! page header's action bar.
//!
//! The action's button opens the dialog, holding the input's form, and its submit runs the action
//! on the action's route. A refused input renders as a page with its errors, which posts back to
//! the same route. Like the confirmation dialog, it needs JavaScript (ADR-0026).
//!
//! The dialog's form shares its page with others, so its fields' DOM ids and its conditions'
//! signals carry the dialog's id, and its searchable choices fetch from the action's own options
//! route.

use tablo_ui::{
    ButtonSize, ButtonVariant, button, dialog, dialog_content, dialog_description, dialog_footer,
    dialog_header, dialog_title,
};
use topcoat::{
    context::Cx,
    runtime::{Event, Signal, expr},
    view::*,
};

use crate::{
    form::FieldErrors,
    panel::actions::MATCHING,
    schema::{Schema, Source},
    topcoat_compat::async_page,
};

/// The dialog asking for the input of one action.
pub(crate) struct InputDialog {
    /// The dialog's DOM id: its trigger opens it, and its fields' ids carry it.
    pub(crate) id: String,
    pub(crate) title: String,
    /// The submit button's label: the action's.
    pub(crate) label: String,
    /// Whether the action confirms: the dialog says it cannot be undone, and its submit renders
    /// destructive and carries the confirmation.
    pub(crate) confirm: bool,
    /// The input's form.
    pub(crate) input: fn() -> Schema,
    /// The action's options route, which its searchable choices fetch from.
    pub(crate) options: String,
}

/// Where the dialog's form posts.
pub(crate) enum Posts {
    /// One target: a header action's, or one record's on its page.
    To(String),
    /// The target the trigger sets: a table row's action, or the bulk bar's with the selection.
    Table {
        target: TableTarget,
        /// The table's selection, which the bulk route reads and a row route ignores.
        selection: Selection,
    },
}

/// The signals a table's trigger writes before opening the dialog of one action.
#[derive(Clone)]
pub(crate) struct TableTarget {
    /// The POST target: a row's route, or the bulk route, which the trigger sets before opening.
    pub(crate) action: Signal<String>,
    /// Whether the dialog runs on the selection.
    pub(crate) bulk: Signal<bool>,
}

impl InputDialog {
    /// Renders the dialog, closed, posting where `posts` says.
    pub(crate) fn render(self, cx: &Cx, posts: Posts) -> BoxView<'_> {
        async_page(async move {
            let Self {
                id,
                title,
                label,
                confirm,
                input,
                options,
            } = self;
            let schema = input();
            let (values, errors) = (schema.defaults(), FieldErrors::new());
            // The fields' ids sit under their own prefix, so a field named `title` never takes
            // the dialog title's id.
            let source = Source::form(&values, &errors)
                .scoped(format!("{id}-field"))
                .options_at(options);
            let fields = schema.render(cx, source).await?;
            let csrf = crate::csrf::current_token(cx);
            let dialog_id = id.clone();
            let title_id = format!("{id}-title");
            let description_id = format!("{id}-description");
            let submit = if confirm {
                ButtonVariant::Destructive
            } else {
                ButtonVariant::Primary
            };
            let table = matches!(posts, Posts::Table { .. });
            // The warning describes the dialog; the selection's size only shows beside it.
            let described = confirm.then(|| description_id.clone());
            let reset = id.clone();
            // A closed dialog keeps what was typed in it, and the next run, maybe on another row,
            // starts blank. Each control then announces a `change`, so a condition follows its
            // blank value.
            let attrs = attributes! {
                cx =>
                id=(id)
                aria-labelledby=(title_id.clone())
                aria-describedby=(described)
                @close=$(|_e: Event| {
                    raw!(
                        "((form) => { \
                            form.reset(); \
                            for (const control of form.elements) { \
                                if (control.name) { \
                                    control.dispatchEvent(new Event('change', { bubbles: true })); \
                                } \
                            } \
                        })(document.getElementById(String(${reset})).querySelector('form'))",
                        (),
                    );
                })
            };
            let (action, selection, count) = match posts {
                Posts::To(url) => (attributes! { cx => action=(url) }, None, None),
                Posts::Table { target, selection } => {
                    let TableTarget { action, bulk } = target;
                    let count = selection.size(cx);
                    (
                        attributes! { cx => :action=$(action.get()) },
                        Some(selection),
                        Some((bulk, count)),
                    )
                }
            };
            // Without the warning, the description holds only the selection's size, so a row's
            // run hides it.
            let description = match &count {
                Some((bulk, _)) if !confirm => {
                    let bulk = bulk.clone();
                    attributes! { cx => id=(description_id) :hidden=$(!bulk.get()) }
                }
                _ => attributes! { cx => id=(description_id) },
            };
            Ok(view! {
                cx =>
                dialog(
                    open: false,
                    attrs: attrs,
                    <form method="post" class="contents" (action)>
                        (crate::csrf::field(cx, &csrf))
                        if confirm {
                            <input type="hidden" name="confirm" value="1">
                        }
                        if let Some(selection) = selection {
                            (selection.inputs(cx))
                        }
                        dialog_content(
                            dialog_header(
                                dialog_title(attrs: attributes! { id=(title_id) }, (title))
                                if confirm || table {
                                    dialog_description(
                                        attrs: description,
                                        if confirm {
                                            (crate::t!(cx, "tablo-confirm-irreversible"))
                                        }
                                        if let Some((bulk, count)) = count {
                                            <span :hidden=$(!bulk.get())>
                                                " "
                                                (crate::t!(cx, "tablo-confirm-selected"))
                                                " "
                                                (count)
                                            </span>
                                        }
                                    )
                                }
                            )
                            <div class="flex flex-col gap-4">(fields)</div>
                            dialog_footer(
                                // Not a submit: Enter in a field submits through the form's
                                // first submit button, which must be the action's.
                                button(
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Md,
                                    attrs: attributes! {
                                        type="button"
                                        @click=$(|_e: Event| {
                                            raw!(
                                                "document.getElementById(String(${dialog_id})).close()",
                                                (),
                                            );
                                        })
                                    },
                                    (crate::t!(cx, "tablo-cancel"))
                                )
                                button(
                                    variant: submit,
                                    size: ButtonSize::Md,
                                    attrs: attributes! { type="submit" },
                                    (label)
                                )
                            )
                        )
                    </form>
                )
            }
            .boxed())
        })
    }
}

/// A table's bulk selection as its write forms post it: the rows checked, or every record the
/// list's query selects.
#[derive(Clone)]
pub(crate) struct Selection {
    /// The checked rows as `,token,` entries, each token's key percent-encoded
    /// ([`bulk_token`](crate::table::bulk_token) spells one).
    pub(crate) keys: Signal<String>,
    /// Whether the selection takes every record `query` selects rather than `keys`'.
    pub(crate) all: Signal<bool>,
    /// The list's URL query, without the leading `?`.
    pub(crate) query: Signal<String>,
}

impl Selection {
    /// The hidden inputs posting the selection: `ids`, and the list's query under
    /// [`MATCHING`] while it takes every record the query selects. A disabled input posts
    /// nothing.
    pub(crate) fn inputs<'a>(&self, cx: &'a Cx) -> BoxView<'a> {
        let (keys, all, query) = (self.keys.clone(), self.all.clone(), self.query.clone());
        view! {
            cx =>
            <input type="hidden" name="ids" :value=$(keys.get())>
            <input
                type="hidden"
                name=(MATCHING)
                :value=$(query.get())
                :disabled=$(!all.get())
            >
        }
        .boxed()
    }

    /// The selection's size: the count of the rows checked, or the words for every matching
    /// record.
    pub(crate) fn size<'a>(&self, cx: &'a Cx) -> BoxView<'a> {
        let (counted, all) = (self.all.clone(), self.all.clone());
        let count = selected_count(cx, self.keys.clone());
        view! {
            cx =>
            <span :hidden=$(counted.get())>(count)</span>
            <span :hidden=$(!all.get())>
                (crate::t!(cx, "tablo-list-all-matching"))
            </span>
        }
        .boxed()
    }
}

/// The size of the table's `selection`, counted as its `,token,` entries.
pub(crate) fn selected_count(cx: &Cx, selection: Signal<String>) -> BoxView<'_> {
    let count = expr!({
        let wire = selection.get();
        raw!(
            "cx.hydrate(String(String(${wire}).split(',').filter(Boolean).length))",
            wire.split(',')
                .filter(|key| !key.is_empty())
                .count()
                .to_string()
        )
    });
    view! { cx => (count) }.boxed()
}

/// A trigger's attributes: it opens the dialog `id`, after pointing a table's dialog at `url`
/// when `table` names its target, on the selection when `bulk`.
pub(crate) fn open_dialog(
    cx: &Cx,
    id: &str,
    table: Option<(TableTarget, String, bool)>,
) -> Attributes {
    let dialog_id = id.to_string();
    let mut attrs = attributes! {
        cx =>
        type="button"
        aria-haspopup="dialog"
        aria-controls=(dialog_id.clone())
    };
    attrs.extend(match table {
        None => attributes! {
            cx =>
            @click=$(|_e: Event| {
                raw!("document.getElementById(String(${dialog_id})).showModal()", ());
            })
        },
        Some((TableTarget { action, bulk }, url, on_selection)) => attributes! {
            cx =>
            @click=$(|_e: Event| {
                bulk.set(on_selection);
                action.set(url.clone());
                raw!("document.getElementById(String(${dialog_id})).showModal()", ());
            })
        },
    });
    attrs
}
