//! The sidebar footer's user menu, and the tenant menu atop the navigation.

use tablo_ui::{
    AvatarSize, FloatingAlign, FloatingSide, SidebarMenuButtonSize, SidebarMenuButtonVariant,
    avatar, avatar_fallback, dropdown_menu, dropdown_menu_content, dropdown_menu_item,
    dropdown_menu_label, dropdown_menu_separator, dropdown_menu_trigger, sidebar_menu,
    sidebar_menu_button_variants, sidebar_menu_item,
};
use topcoat::{
    context::Cx,
    icon::{IconData, icon},
    view::{BoxView, ViewExt, attributes, view},
};

use super::Theme;
use crate::{Panel, auth::PanelUser, navigation::NavigationItem, tenancy::Membership};

/// The DOM id of the user menu's panel.
const USER_MENU: &str = "user-menu";

/// The DOM id of the theme menu's panel, which a panel without sessions shows.
const THEME_MENU: &str = "theme-menu";

/// The DOM id of the tenant menu's panel.
const TENANT_MENU: &str = "tenant-menu";

/// Up to two initials of `name`, from its first two words: `Ada Lovelace` is `AL`.
pub(super) fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

impl Panel {
    /// The user menu: the user's avatar, name and subtitle, opening the theme choice, the app's
    /// `items`, and Sign out, which posts to `logout` with `csrf`.
    pub(super) fn user_menu<'a>(
        cx: &'a Cx,
        user: &dyn PanelUser,
        items: &[NavigationItem],
        logout: String,
        csrf: &str,
    ) -> BoxView<'a> {
        let name = user.display_name().to_string();
        let subtitle = user.subtitle().map(str::to_string);
        let initials = initials(&name);
        let themes = theme_choices(cx);
        let links: Vec<BoxView<'a>> = items
            .iter()
            .filter_map(|item| {
                let link = crate::navigation::runtime_link(cx, item.url()?);
                let label = item.label.clone();
                let data = item.icon.clone();
                Some(
                    view! {
                        cx =>
                        <a (link) class=(crate::navigation::MENU_LINK_CLASS)>
                            if let Some(data) = data {
                                icon(data: data)
                            }
                            (label)
                        </a>
                    }
                    .boxed(),
                )
            })
            .collect();
        let csrf = crate::csrf::field(cx, csrf);
        let trigger_class = sidebar_menu_button_variants(
            SidebarMenuButtonVariant::Default,
            SidebarMenuButtonSize::Lg,
        );
        let menu_name = name.clone();
        let menu_subtitle = subtitle.clone();
        view! {
            cx =>
            sidebar_menu(
                sidebar_menu_item(
                    dropdown_menu(
                        dropdown_menu_trigger(
                            target: USER_MENU,
                            attrs: attributes! {
                                class=(trigger_class)
                                data-user-menu=""
                                aria-label="Account"
                            },
                            avatar(
                                size: AvatarSize::Sm,
                                attrs: attributes! { class="rounded-lg" },
                                avatar_fallback(
                                    attrs: attributes! { class="rounded-lg" },
                                    (initials)
                                )
                            )
                            <span
                                class="grid min-w-0 flex-1 text-left leading-tight md:group-data-[collapsible=icon]/sidebar:hidden"
                            >
                                <span class="truncate font-medium">(name)</span>
                                if let Some(subtitle) = subtitle {
                                    <span class="truncate text-xs text-muted-foreground">
                                        (subtitle)
                                    </span>
                                }
                            </span>
                            icon(
                                data: tablo_ui::icons::CHEVRONS_UP_DOWN,
                                attrs: attributes! {
                                    class="ml-auto size-4 md:group-data-[collapsible=icon]/sidebar:hidden"
                                }
                            )
                        )
                        dropdown_menu_content(
                            id: USER_MENU,
                            side: FloatingSide::Top,
                            align: FloatingAlign::Start,
                            attrs: attributes! { class="min-w-56" },
                            dropdown_menu_label(
                                <span class="grid leading-tight">
                                    <span class="truncate font-medium text-foreground">
                                        (menu_name)
                                    </span>
                                    if let Some(subtitle) = menu_subtitle {
                                        <span class="truncate text-xs font-normal">
                                            (subtitle)
                                        </span>
                                    }
                                </span>
                            )
                            dropdown_menu_separator()
                            (themes)
                            if !links.is_empty() {
                                dropdown_menu_separator()
                                for link in links {
                                    (link)
                                }
                            }
                            dropdown_menu_separator()
                            <form method="post" action=(logout)>
                                (csrf)
                                dropdown_menu_item(
                                    attrs: attributes! { type="submit" },
                                    icon(data: tablo_ui::icons::LOG_OUT)
                                    "Sign out"
                                )
                            </form>
                        )
                    )
                )
            )
        }
        .boxed()
    }

    /// The theme menu, in the user menu's place when the panel has no session to show: the theme
    /// choice alone.
    pub(super) fn theme_menu(cx: &Cx) -> BoxView<'_> {
        let themes = theme_choices(cx);
        let trigger_class = sidebar_menu_button_variants(
            SidebarMenuButtonVariant::Default,
            SidebarMenuButtonSize::Md,
        );
        view! {
            cx =>
            sidebar_menu(
                sidebar_menu_item(
                    dropdown_menu(
                        dropdown_menu_trigger(
                            target: THEME_MENU,
                            attrs: attributes! {
                                class=(trigger_class)
                                data-theme-menu=""
                                aria-label="Theme"
                            },
                            icon(data: tablo_ui::icons::SUN)
                            <span
                                class="truncate md:group-data-[collapsible=icon]/sidebar:hidden"
                            >
                                "Theme"
                            </span>
                        )
                        dropdown_menu_content(
                            id: THEME_MENU,
                            side: FloatingSide::Top,
                            align: FloatingAlign::Start,
                            attrs: attributes! { class="min-w-40" },
                            (themes)
                        )
                    )
                )
            )
        }
        .boxed()
    }

    /// The tenant menu atop the navigation: the tenant the request acts for, opening the user's
    /// `tenants`, each a form posting to the tenant-switch route, then the tenant registration
    /// page when the panel offers one. A user with fewer than two tenants and nowhere to create
    /// another has nothing to choose, and gets nothing.
    pub(super) fn tenant_menu<'a>(cx: &'a Cx, tenants: &[Membership], csrf: &str) -> BoxView<'a> {
        let register = crate::auth::tenant_registration_offered(cx)
            .then(|| crate::navigation::runtime_link(cx, &crate::auth::register_tenant_url(cx)));
        if tenants.len() < 2 && register.is_none() {
            return ().boxed();
        }
        let current = crate::auth::membership(cx);
        let label = current.map_or_else(|| "Select a tenant".to_string(), |m| m.name.clone());
        let current = current.map(|membership| membership.tenant);
        let action = crate::auth::tenant_url(cx);
        let choices: Vec<BoxView<'a>> = tenants
            .iter()
            .map(|membership| {
                let selected = current == Some(membership.tenant);
                let value = membership.tenant.to_string();
                let name = membership.name.clone();
                let action = action.clone();
                let csrf = crate::csrf::field(cx, csrf);
                view! {
                    cx =>
                    <form method="post" action=(action)>
                        (csrf)
                        <input
                            type="hidden"
                            name=(crate::auth::TENANT_FIELD)
                            value=(value)
                        >
                        dropdown_menu_item(
                            attrs: attributes! { type="submit" aria-current=(selected.then_some("true")) },
                            <span class="flex-1 truncate">(name)</span>
                            if selected {
                                icon(data: tablo_ui::icons::CHECK)
                            }
                        )
                    </form>
                }
                .boxed()
            })
            .collect();
        let trigger_class = sidebar_menu_button_variants(
            SidebarMenuButtonVariant::Outline,
            SidebarMenuButtonSize::Md,
        );
        view! {
            cx =>
            <div
                class="px-2 pt-2 md:group-data-[collapsible=icon]/sidebar:hidden"
                data-tenant-switcher=""
            >
                dropdown_menu(
                    dropdown_menu_trigger(
                        target: TENANT_MENU,
                        attrs: attributes! { class=(trigger_class) aria-label="Switch tenant" },
                        <span class="flex-1 truncate">(label)</span>
                        icon(
                            data: tablo_ui::icons::CHEVRONS_UP_DOWN,
                            attrs: attributes! { class="size-4 text-muted-foreground" }
                        )
                    )
                    dropdown_menu_content(
                        id: TENANT_MENU,
                        align: FloatingAlign::Start,
                        attrs: attributes! { class="min-w-56" },
                        dropdown_menu_label("Tenants")
                        for choice in choices {
                            (choice)
                        }
                        if let Some(register) = register {
                            dropdown_menu_separator()
                            <a
                                (register)
                                class=(crate::navigation::MENU_LINK_CLASS)
                                data-tenant-registration=""
                            >
                                icon(data: tablo_ui::icons::PLUS)
                                "Create tenant"
                            </a>
                        }
                    )
                )
            </div>
        }
        .boxed()
    }
}

/// The theme choice: Light, Dark and System, the current one checked.
fn theme_choices(cx: &Cx) -> BoxView<'_> {
    let theme = Theme::current(cx);
    let items: Vec<BoxView<'_>> = [
        (Theme::Light, "Light", tablo_ui::icons::SUN),
        (Theme::Dark, "Dark", tablo_ui::icons::MOON),
        (Theme::System, "System", tablo_ui::icons::MONITOR),
    ]
    .into_iter()
    .map(|(choice, label, data)| theme_item(cx, choice, theme == choice, label, data))
    .collect();
    view! {
        cx =>
        <div role="radiogroup" aria-label="Theme">
            for item in items {
                (item)
            }
        </div>
    }
    .boxed()
}

/// One theme choice: a radio the theme script checks when chosen.
fn theme_item<'a>(
    cx: &'a Cx,
    choice: Theme,
    checked: bool,
    label: &'static str,
    data: IconData,
) -> BoxView<'a> {
    view! {
        cx =>
        dropdown_menu_item(
            attrs: attributes! {
                role="radio"
                aria-checked=(if checked { "true" } else { "false" })
                data-theme-set=(choice.name())
            },
            icon(data: data)
            (label)
            icon(
                data: tablo_ui::icons::CHECK,
                attrs: attributes! { class="ml-auto opacity-0 in-aria-checked:opacity-100" }
            )
        )
    }
    .boxed()
}

#[cfg(test)]
mod tests;
