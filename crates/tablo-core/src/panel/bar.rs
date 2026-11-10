//! The action buttons a page header carries: a list's or a page's header actions, and one
//! record's custom actions and Delete on its detail and edit pages.

use tablo_ui::{
    ButtonSize, ButtonVariant, alert_dialog, button, dialog_content, dialog_description,
    dialog_footer, dialog_header, dialog_title,
};
use topcoat::{
    context::Cx,
    icon::{IconData, icon},
    runtime::Event,
    view::*,
};

use crate::{
    Color, HeaderActions,
    panel::{
        input::{InputDialog, Posts, open_dialog},
        url::{
            action_options_url, bulk_action_url, delete_action_url, row_action_url, with_return,
        },
    },
    policy::Ability,
    resource::{HeaderEntry, InputSpec, Mounted, Places, Resource},
    t,
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
    confirm: Option<String>,
    /// The icon before the label.
    icon: Option<IconData>,
    /// The count a badge shows after the label, for a header action declared with one.
    badge: Option<String>,
    /// The colour the button is tinted in.
    color: Color,
    /// The action's input, when it asks for one.
    input: Option<Input>,
    /// The action's name and why the button shows disabled, when the record's state refuses it.
    disabled: Option<(&'static str, String)>,
    /// The input the button posts as hidden fields, for an action whose input the page fills.
    preset: Vec<(String, String)>,
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
    badges: &[(&'static str, String)],
) -> ActionBar {
    let buttons = actions
        .entries()
        .iter()
        .filter(|action| allowed(action) && (action.can_run)(cx))
        .map(|action| ActionButton {
            label: (action.label)().localize(cx),
            url: bulk_action_url(url.trim_end_matches('/'), action.name),
            // An action with input confirms in its input dialog, which its button opens.
            confirm: (action.confirm && !action.input.takes_input)
                .then(|| (action.prompt)().localize(cx)),
            icon: (action.icon)(),
            badge: badges
                .iter()
                .find(|(name, _)| *name == action.name)
                .map(|(_, count)| count.clone()),
            color: action.color,
            input: Input::of(action.input, action.confirm, url, action.name),
            disabled: None,
            preset: Vec::new(),
        })
        .collect();
    ActionBar { buttons }
}

/// The button of the header `action` under `url`, posting `preset` as its input instead of asking
/// for it, and confirming first when the action does; empty when its
/// [`can_run`](crate::HeaderAction::can_run) refuses the request.
pub(crate) fn preset_button(
    cx: &Cx,
    url: &str,
    action: &HeaderEntry,
    preset: Vec<(String, String)>,
) -> ActionBar {
    if !(action.can_run)(cx) {
        return ActionBar::default();
    }
    let button = ActionButton {
        label: (action.label)().localize(cx),
        url: bulk_action_url(url.trim_end_matches('/'), action.name),
        confirm: action.confirm.then(|| (action.prompt)().localize(cx)),
        icon: (action.icon)(),
        badge: None,
        color: action.color,
        input: None,
        disabled: None,
        preset,
    };
    ActionBar {
        buttons: vec![button],
    }
}

/// The bar of `record`'s actions placed at `place`, each landing back on `here`, then its Delete,
/// which lands on the list: each one the policy allows on the record, as its row would offer it.
/// A trashed record offers its restore and force delete only, and the force delete lands on the
/// list too.
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
        .filter(|action| action.places.contains(place) && resource.can(cx, action.resource_wide))
        .filter_map(|action| {
            let disabled = if action.allows(resource, cx, record) {
                None
            } else {
                let reason = action.disabled_on(resource, cx, record)?;
                Some((action.name, reason.localize(cx)))
            };
            let url = row_action_url(&resource.url, &key, action.name);
            Some(ActionButton {
                label: (action.label_for)(cx, record).localize(cx),
                url: if action.removes {
                    url
                } else {
                    with_return(&url, here)
                },
                confirm: (action.confirm && !action.input.takes_input)
                    .then(|| (action.prompt)(false).localize(cx)),
                icon: (action.icon_for)(cx, record)
                    .or(action.removes.then_some(tablo_ui::icons::TRASH)),
                color: action.color,
                badge: None,
                input: Input::of(action.input, action.confirm, &resource.url, action.name),
                disabled,
                preset: Vec::new(),
            })
        })
        .collect();
    if !resource.is_trashed(record)
        && resource.can(cx, Ability::DeleteAny)
        && resource.can(cx, Ability::Delete(record))
    {
        buttons.push(ActionButton {
            label: t!(cx, "tablo-action-delete"),
            url: delete_action_url(&resource.url, &key),
            confirm: Some(t!(cx, "tablo-confirm-delete-record")),
            icon: Some(tablo_ui::icons::TRASH),
            color: Color::Danger,
            badge: None,
            input: None,
            disabled: None,
            preset: Vec::new(),
        });
    }
    ActionBar { buttons }
}

impl ActionBar {
    /// One button labelled `label`, whose dialog asks for `input` and posts it to `url`,
    /// searching its choices' options at `options`.
    pub(crate) fn input(label: String, url: String, input: InputSpec, options: String) -> Self {
        Self {
            buttons: vec![ActionButton {
                label,
                url,
                confirm: None,
                icon: None,
                color: Color::Gray,
                badge: None,
                input: Some(Input {
                    spec: input,
                    confirm: false,
                    options,
                }),
                disabled: None,
                preset: Vec::new(),
            }],
        }
    }

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
    /// asking for input opens the dialog `id` holding its input form.
    fn render<'a>(self, cx: &'a Cx, csrf: &str, id: String) -> BoxView<'a> {
        let token = crate::csrf::field(cx, csrf);
        let Self {
            label,
            url,
            confirm,
            icon: face_icon,
            badge,
            color,
            input,
            disabled,
            preset,
        } = self;
        let preset = view! {
            for (name, value) in preset {
                <input type="hidden" name=(name) value=(value)>
            }
        }
        .boxed();
        // A tinted button is outlined like its neighbors, so its colour marks it without
        // outweighing the page's primary action.
        let variant = ButtonVariant::Outline;
        let tone = color.button_tone();
        let confirm_label = label.clone();
        let face = view! {
            if let Some(data) = face_icon {
                icon(data: data)
            }
            (label)
            if let Some(count) = badge {
                <span
                    data-action-badge="count"
                    class="rounded-full bg-muted px-1.5 text-xs font-medium tabular-nums"
                >
                    (count)
                </span>
            }
        }
        .boxed();
        if let Some((name, reason)) = disabled {
            return view! {
                button(
                    variant: variant,
                    size: ButtonSize::Md,
                    attrs: disabled_trigger(cx, name, &reason, tone),
                    (face)
                )
            }
            .boxed();
        }
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
            return view! {
                button(
                    variant: variant,
                    size: ButtonSize::Md,
                    attrs: {
                        let mut attrs = open_dialog(cx, &id, None);
                        attrs.extend(attributes! { class=(tone) });
                        attrs
                    },
                    (face)
                )
                (dialog)
            }
            .boxed();
        }
        let Some(title) = confirm else {
            return view! {
                <form method="post" action=(url) class="contents">
                    (token)
                    (preset)
                    button(
                        variant: variant,
                        size: ButtonSize::Md,
                        attrs: attributes! { type="submit" class=(tone) },
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
            button(
                variant: variant,
                size: ButtonSize::Md,
                attrs: attributes! {
                    type="button"
                    class=(tone)
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
                    (preset)
                    <input type="hidden" name="confirm" value="1">
                    dialog_content(
                        dialog_header(
                            dialog_title(attrs: attributes! { id=(title_id) }, (title))
                            dialog_description(
                                attrs: attributes! { id=(description_id) },
                                (t!(cx, "tablo-confirm-irreversible"))
                            )
                        )
                        dialog_footer(
                            button(
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Md,
                                attrs: attributes! { type="submit" formmethod="dialog" },
                                (t!(cx, "tablo-cancel"))
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

/// The attributes of the button of the action `name`, which the record's state refuses: it posts
/// nothing, stays focusable so a reader hears `reason`, and shows `reason` on hover. Its `tone`
/// joins the disabled classes, since a second `class` attribute would replace them.
pub(crate) fn disabled_trigger(
    cx: &Cx,
    name: &str,
    reason: &str,
    tone: Option<&str>,
) -> Attributes {
    let class = tone.map_or_else(
        || "cursor-not-allowed opacity-50".to_string(),
        |tone| format!("cursor-not-allowed opacity-50 {tone}"),
    );
    attributes! {
        type="button"
        aria-disabled="true"
        title=(reason.to_string())
        data-disabled-action=(name.to_string())
        class=(class)
    }
}
