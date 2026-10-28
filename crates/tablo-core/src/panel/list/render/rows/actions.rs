//! A row's actions: its custom actions, View, Edit and Delete, inline or behind one menu.

use tablo_ui::{
    ButtonSize, ButtonVariant, FloatingAlign, button, button_variants, dropdown_menu,
    dropdown_menu_content, dropdown_menu_item, dropdown_menu_separator, dropdown_menu_trigger,
    table_cell,
};
use topcoat::{context::Cx, icon::icon, view::*};

use super::{
    super::{
        CHROME_CELL_CLASS, ROW_DELETE_CLASS, ROW_ICON_CLASS,
        dialog::{delete_confirmation, input_trigger, run_confirmation, write_trigger},
    },
    RowButton, RowChrome, RowView,
};
use crate::{
    dom,
    panel::bar::{DESTRUCTIVE_TONE, disabled_trigger},
    t,
};

/// The row's actions cell: its custom actions, then View, Edit and Delete, each only when the
/// row's policy allows it.
pub(super) fn render_actions<'a>(cx: &'a Cx, row: &RowView<'a>, chrome: &RowChrome) -> BoxView<'a> {
    if chrome.action_menu {
        return render_action_menu(cx, row, chrome);
    }
    let view_url = row.view_url.clone();
    let edit_url = row.edit_url.clone();
    let described = chrome.row_id(&row.key);
    let custom: Vec<BoxView<'a>> = row
        .custom
        .iter()
        .map(|action| {
            let mut attrs = trigger(cx, chrome, action);
            let tone = action.destructive.then_some(DESTRUCTIVE_TONE);
            attrs.extend(attributes! { cx => class=(tone) aria-describedby=(described.clone()) });
            let (label, face) = (action.label.clone(), action.icon.clone());
            view! {
                cx =>
                button(
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    attrs: attrs,
                    if let Some(data) = face {
                        icon(data: data)
                    }
                    (label)
                )
            }
            .boxed()
        })
        .collect();
    let delete = row.delete_action.clone().map(|action| {
        let confirm = Some(delete_confirmation(cx, false));
        let mut attrs = write_trigger(cx, &chrome.form, &chrome.signals, action, confirm, false);
        attrs.extend(attributes! {
            cx =>
            class=(ROW_DELETE_CLASS)
            aria-label=(t!(cx, "tablo-action-delete"))
            aria-describedby=(described.clone())
            title=(t!(cx, "tablo-action-delete"))
            data-row-action=(dom::row::DELETE)
        });
        attrs
    });
    view! {
        cx =>
        table_cell(
            attrs: attributes! { class=(CHROME_CELL_CLASS) },
            <div class="relative z-10 flex items-center justify-end gap-1">
                for control in custom {
                    (control)
                }
                if let Some(url) = view_url {
                    <a
                        (crate::navigation::runtime_link(cx, &url))
                        class=(ROW_ICON_CLASS)
                        aria-label=(t!(cx, "tablo-action-view"))
                        aria-describedby=(described.clone())
                        title=(t!(cx, "tablo-action-view"))
                        data-row-action=(dom::row::VIEW)
                    >
                        icon(data: tablo_ui::icons::EYE)
                    </a>
                }
                if let Some(url) = edit_url {
                    <a
                        (crate::navigation::runtime_link(cx, &url))
                        class=(ROW_ICON_CLASS)
                        aria-label=(t!(cx, "tablo-action-edit"))
                        aria-describedby=(described.clone())
                        title=(t!(cx, "tablo-action-edit"))
                        data-row-action=(dom::row::EDIT)
                    >
                        icon(data: tablo_ui::icons::PENCIL)
                    </a>
                }
                if let Some(attrs) = delete {
                    <button (attrs)>icon(data: tablo_ui::icons::TRASH)</button>
                }
            </div>
        )
    }
    .boxed()
}

/// The attributes of a custom action's button in a row: disabled with its reason, opening its
/// input dialog, or writing.
fn trigger(cx: &Cx, chrome: &RowChrome, action: &RowButton) -> Attributes {
    if let Some(reason) = &action.disabled {
        return disabled_trigger(cx, action.name, reason);
    }
    match action.input {
        Some(name) => input_trigger(
            cx,
            &chrome.form,
            &chrome.signals,
            name,
            action.url.clone(),
            false,
        ),
        None => write_trigger(
            cx,
            &chrome.form,
            &chrome.signals,
            action.url.clone(),
            action
                .confirm
                .as_deref()
                .map(|prompt| run_confirmation(cx, prompt)),
            false,
        ),
    }
}

/// The row's actions behind one button: its custom actions, View and Edit, then Delete after a
/// separator, each only when the row's policy allows it. A row allowing none keeps an empty cell.
fn render_action_menu<'a>(cx: &'a Cx, row: &RowView<'a>, chrome: &RowChrome) -> BoxView<'a> {
    let described = chrome.row_id(&row.key);
    let menu = format!("{described}-actions");
    // Closes the menu as the item runs; a submitting item navigates instead.
    let closes = |attrs: &mut Attributes| {
        attrs.extend(attributes! {
            cx =>
            popovertarget=(menu.clone())
            popovertargetaction="hide"
            aria-describedby=(described.clone())
        });
    };
    let mut items: Vec<BoxView<'a>> = row
        .custom
        .iter()
        .map(|action| {
            let mut attrs = trigger(cx, chrome, action);
            if action.disabled.is_none() {
                closes(&mut attrs);
            }
            if action.destructive {
                attrs.extend(attributes! { cx => class="text-destructive" });
            }
            let (label, face) = (action.label.clone(), action.icon.clone());
            view! {
                cx =>
                dropdown_menu_item(
                    attrs: attrs,
                    if let Some(data) = face {
                        icon(data: data)
                    }
                    (label)
                )
            }
            .boxed()
        })
        .collect();
    for (url, label, data, hook) in [
        (
            row.view_url.clone(),
            t!(cx, "tablo-action-view"),
            tablo_ui::icons::EYE,
            dom::row::VIEW,
        ),
        (
            row.edit_url.clone(),
            t!(cx, "tablo-action-edit"),
            tablo_ui::icons::PENCIL,
            dom::row::EDIT,
        ),
    ] {
        if let Some(url) = url {
            let link = crate::navigation::runtime_link(cx, &url);
            let described = described.clone();
            items.push(
                view! {
                    cx =>
                    <a
                        (link)
                        class=(crate::navigation::MENU_LINK_CLASS)
                        aria-label=(label.clone())
                        aria-describedby=(described)
                        data-row-action=(hook)
                    >
                        icon(data: data)
                        (label)
                    </a>
                }
                .boxed(),
            );
        }
    }
    let delete = row.delete_action.clone().map(|action| {
        let confirm = Some(delete_confirmation(cx, false));
        let mut attrs = write_trigger(cx, &chrome.form, &chrome.signals, action, confirm, false);
        closes(&mut attrs);
        attrs.extend(attributes! {
            cx =>
            class="text-destructive"
            aria-label=(t!(cx, "tablo-action-delete"))
            data-row-action=(dom::row::DELETE)
        });
        attrs
    });
    if items.is_empty() && delete.is_none() {
        let empty = attributes! { cx => class=(CHROME_CELL_CLASS) };
        return view! { cx => table_cell(attrs: empty) }.boxed();
    }
    let separate = !items.is_empty() && delete.is_some();
    let trigger_class = button_variants(ButtonVariant::Ghost, ButtonSize::Icon);
    view! {
        cx =>
        table_cell(
            attrs: attributes! { class=(CHROME_CELL_CLASS) },
            <div class="relative z-10 flex justify-end">
                dropdown_menu(
                    dropdown_menu_trigger(
                        target: menu.as_str(),
                        attrs: attributes! {
                            class=(trigger_class)
                            aria-label=(t!(cx, "tablo-list-actions"))
                            aria-describedby=(described.clone())
                        },
                        icon(data: tablo_ui::icons::ELLIPSIS)
                    )
                    dropdown_menu_content(
                        id: menu.as_str(),
                        align: FloatingAlign::End,
                        for item in items {
                            (item)
                        }
                        if separate {
                            dropdown_menu_separator()
                        }
                        if let Some(attrs) = delete {
                            dropdown_menu_item(
                                attrs: attrs,
                                icon(data: tablo_ui::icons::TRASH)
                                (t!(cx, "tablo-action-delete"))
                            )
                        }
                    )
                )
            </div>
        )
    }
    .boxed()
}
