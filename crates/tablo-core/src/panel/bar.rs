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
    resource::{HeaderEntry, InputSpec, Mounted, Places, Resource},
    table::{
        InputDialog, Posts, action_options_url, bulk_action_url, delete_action_url, open_dialog,
        row_action_url, with_return,
    },
};

/// The buttons of one page header, each a POST of its own form.
#[derive(Default)]
pub(crate) struct ActionBar {
    buttons: Vec<ActionButton>,
}

/// One button: a direct POST, one a confirmation dialog asks for first, or one whose dialog asks
/// for the action's input.
struct ActionButton {
    label: String,
    /// The POST target, with its `?return=` when the write lands back on this page.
    url: String,
    /// The dialog's title, when the button confirms first.
    confirm: Option<&'static str>,
    delete: bool,
    /// The action's input, when it asks for one.
    input: Option<Input>,
}

/// What an action's input dialog renders.
struct Input {
    spec: InputSpec,
    /// Whether the action confirms, which its dialog then says.
    confirm: bool,
    /// The action's options route.
    options: String,
}

impl Input {
    /// The input of an action that `takes_input`, confirming when `confirm`, whose options route
    /// sits under `url` as `name`'s.
    fn of(spec: InputSpec, confirm: bool, url: &str, name: &str) -> Option<Self> {
        spec.takes_input.then(|| Self {
            spec,
            confirm,
            options: action_options_url(url, name),
        })
    }
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
            // An action with input confirms in its input dialog, which its button opens.
            confirm: (action.confirm && !action.input.takes_input).then_some("Run this action?"),
            delete: false,
            input: Input::of(action.input, action.confirm, url, action.name),
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
            input: Input::of(action.input, action.confirm, &resource.url, action.name),
        })
        .collect();
    if resource.can(cx, Ability::DeleteAny) && resource.can(cx, Ability::Delete(record)) {
        buttons.push(ActionButton {
            label: "Delete".to_string(),
            url: delete_action_url(&resource.url, &key),
            confirm: Some("Delete this record?"),
            delete: true,
            input: None,
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
            .map(|action| {
                let id = dialog_id(&action.url, action.input.is_some());
                action.render(cx, &csrf, id)
            })
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

/// The DOM id of the dialog confirming a POST to `url`, or asking for its `input`: a page that
/// renders several bars, or one twice, never opens one button's dialog from another's.
fn dialog_id(url: &str, input: bool) -> String {
    use std::hash::{BuildHasher, BuildHasherDefault, DefaultHasher};
    let hash = BuildHasherDefault::<DefaultHasher>::default().hash_one(url);
    let kind = if input { "input" } else { "confirm" };
    format!("action-{kind}-{hash:016x}")
}

impl ActionButton {
    /// A direct button submits its own form. A confirming one opens the alert dialog `id`, whose
    /// form carries the `confirm=1` marker the handlers require; Cancel and Escape close it. One
    /// asking for input opens the dialog `id` holding its input form, and submits its own form
    /// to the input page without scripts.
    fn render<'a>(self, cx: &'a Cx, csrf: &str, id: String) -> BoxView<'a> {
        let token = crate::csrf::field(cx, csrf);
        let Self {
            label,
            url,
            confirm,
            delete,
            input,
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
        if let Some(Input {
            spec,
            confirm,
            options,
        }) = input
        {
            let dialog = InputDialog {
                id: id.clone(),
                title: confirm_label.clone(),
                label: confirm_label,
                confirm,
                input: spec.schema,
                options,
            }
            .render(cx, Posts::To(url.clone()));
            let open = open_dialog(cx, &id, None);
            return view! {
                cx =>
                <form method="post" action=(url) class="contents">
                    (token)
                    button(
                        variant: variant,
                        size: ButtonSize::Md,
                        attrs: attributes! { type="submit" (open) },
                        (face)
                    )
                </form>
                (dialog)
            }
            .boxed();
        }
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
