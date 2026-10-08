//! The action buttons a page header carries: a list's or a page's header actions, and one
//! record's custom actions and Delete on its detail and edit pages.

use tablo_ui::{
    ButtonSize, ButtonVariant, alert_dialog, button, dialog_content, dialog_description,
    dialog_footer, dialog_header, dialog_title,
};
use topcoat::{context::Cx, icon::icon, runtime::Event, view::*};

use crate::{
    HeaderActions,
    policy::Ability,
    resource::{HeaderEntry, Mounted, Places, Resource},
    table::{bulk_action_url, delete_action_url, row_action_url, with_return},
};

/// The buttons of one page header, each a POST of its own form.
#[derive(Default)]
pub(crate) struct ActionBar {
    buttons: Vec<ActionButton>,
}

/// One button: a direct POST, or one a confirmation dialog asks for first.
struct ActionButton {
    label: String,
    /// The POST target, with its `?return=` when the write lands back on this page.
    url: String,
    /// The dialog's title, when the button confirms first.
    confirm: Option<&'static str>,
    delete: bool,
}

/// The bar of `actions` under `url`, a list's or a page's, each the request may run: one that
/// `allowed` and its own [`can_run`](crate::HeaderAction::can_run) admit.
pub(crate) fn header_bar(
    cx: &Cx,
    url: &str,
    actions: &HeaderActions,
    allowed: impl Fn(&HeaderEntry) -> bool,
) -> ActionBar {
    let buttons = actions
        .entries()
        .iter()
        .filter(|action| allowed(action) && (action.can_run)(cx))
        .map(|action| ActionButton {
            label: (action.label)(cx),
            url: bulk_action_url(url.trim_end_matches('/'), action.name),
            // An action with input confirms on its input page, which its button opens.
            confirm: (action.confirm && !action.input.takes_input).then_some("Run this action?"),
            delete: false,
        })
        .collect();
    ActionBar { buttons }
}

/// The bar of `record`'s actions placed at `place`, each landing back on `here`, then its Delete,
/// which lands on the list: each one the policy allows on the record, as its row would offer it.
pub(crate) fn record_bar<R: Resource>(
    cx: &Cx,
    resource: &Mounted<R>,
    record: &R::Model,
    place: Places,
    here: &str,
) -> ActionBar {
    // A composite key resolves as no URL id: its record has no action route.
    if !resource.table.is_addressable() || !resource.can(cx, Ability::View(record)) {
        return ActionBar::default();
    }
    let key = resource.table.key_of(record);
    let mut buttons: Vec<ActionButton> = resource
        .actions
        .entries()
        .iter()
        .filter(|action| {
            action.places.contains(place)
                && resource.can(cx, action.resource_wide)
                && (action.can_run)(resource, cx, record)
        })
        .map(|action| ActionButton {
            label: (action.label)(cx),
            url: with_return(&row_action_url(&resource.url, &key, action.name), here),
            confirm: (action.confirm && !action.input.takes_input).then_some("Run this action?"),
            delete: false,
        })
        .collect();
    if resource.can(cx, Ability::DeleteAny) && resource.can(cx, Ability::Delete(record)) {
        buttons.push(ActionButton {
            label: "Delete".to_string(),
            url: delete_action_url(&resource.url, &key),
            confirm: Some("Delete this record?"),
            delete: true,
        });
    }
    ActionBar { buttons }
}

impl ActionBar {
    /// Sends each button's write back to `target`.
    pub(crate) fn returning_to(mut self, target: &str) -> Self {
        for action in &mut self.buttons {
            action.url = with_return(&action.url, target);
        }
        self
    }

    /// Whether the bar holds no button.
    pub(crate) fn is_empty(&self) -> bool {
        self.buttons.is_empty()
    }

    /// Renders each button as its own form for a page header's
    /// [`page_actions`](tablo_ui::page_actions), and a confirming one beside its dialog.
    pub(crate) fn render(self, cx: &Cx) -> BoxView<'_> {
        if self.buttons.is_empty() {
            return ().boxed();
        }
        let csrf = crate::csrf::ensure_token(cx);
        let controls: Vec<BoxView<'_>> = self
            .buttons
            .into_iter()
            .enumerate()
            .map(|(index, action)| action.render(cx, &csrf, format!("header-action-{index}")))
            .collect();
        view! {
            cx =>
            for control in controls {
                (control)
            }
        }
        .boxed()
    }
}

impl ActionButton {
    /// A direct button submits its own form. A confirming one opens the alert dialog `id`, whose
    /// form carries the `confirm=1` marker the handlers require; Cancel and Escape close it.
    fn render<'a>(self, cx: &'a Cx, csrf: &str, id: String) -> BoxView<'a> {
        let token = crate::csrf::field(cx, csrf);
        let Self {
            label,
            url,
            confirm,
            delete,
        } = self;
        let variant = if delete {
            ButtonVariant::Destructive
        } else {
            ButtonVariant::Outline
        };
        let confirm_label = label.clone();
        let face = view! {
            cx =>
            if delete {
                icon(data: tablo_ui::icons::TRASH)
            }
            (label)
        }
        .boxed();
        let Some(title) = confirm else {
            return view! {
                cx =>
                <form method="post" action=(url) class="contents">
                    (token)
                    button(
                        variant: variant,
                        size: ButtonSize::Md,
                        attrs: attributes! { type="submit" },
                        (face)
                    )
                </form>
            }
            .boxed();
        };
        let title_id = format!("{id}-title");
        let description_id = format!("{id}-description");
        let dialog = id.clone();
        view! {
            cx =>
            button(
                variant: variant,
                size: ButtonSize::Md,
                attrs: attributes! {
                    type="button"
                    aria-haspopup="dialog"
                    @click=$(|_e: Event| {
                        raw!(
                            "document.getElementById(String(${dialog})).showModal()",
                            (),
                        );
                    })
                },
                (face)
            )
            alert_dialog(
                open: false,
                attrs: attributes! {
                    id=(id)
                    aria-labelledby=(title_id.clone())
                    aria-describedby=(description_id.clone())
                },
                <form method="post" action=(url) class="contents">
                    (token)
                    <input type="hidden" name="confirm" value="1">
                    dialog_content(
                        dialog_header(
                            dialog_title(attrs: attributes! { id=(title_id) }, (title))
                            dialog_description(
                                attrs: attributes! { id=(description_id) },
                                "This action cannot be undone."
                            )
                        )
                        dialog_footer(
                            button(
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Md,
                                attrs: attributes! { type="submit" formmethod="dialog" },
                                "Cancel"
                            )
                            button(
                                variant: ButtonVariant::Destructive,
                                size: ButtonSize::Md,
                                attrs: attributes! { type="submit" },
                                (confirm_label)
                            )
                        )
                    )
                </form>
            )
        }
        .boxed()
    }
}
