//! Renders the shell framing every admin page.

use http::header::COOKIE;
use topcoat::{
    Result,
    asset::Asset,
    context::{Cx, try_request_context},
    font::Font,
    icon::icon,
    router::Slot,
    runtime::{Event, Signal, signal},
    view::{
        Attributes, BoxView, Child, HoistView, View, ViewExt, attributes, internal::ThenView, view,
    },
};

use super::{Panel, state::current};
use crate::{
    notification::{LiveToast, live_toast, live_toaster, take_notification},
    resource::NavigationItem,
};

/// `extra` plus the attribute that sends a sidebar link through runtime
/// navigation. The menu button writes the `href` itself, so this carries none.
fn sidebar_link(cx: &Cx, mut extra: Attributes) -> Attributes {
    let mut attrs = crate::resource::runtime_link(cx, "");
    attrs.remove("href");
    extra.extend(attrs);
    extra
}

/// The value of the request's `name` cookie.
///
/// Parsed from the raw `Cookie` header on purpose: `topcoat::cookie::cookies`
/// panics when the cookie router layer is absent (tests, minimal routers), and
/// the shell must render everywhere.
fn request_cookie(cx: &Cx, name: &str) -> Option<String> {
    let header = try_request_context::<http::request::Parts>(cx)?
        .headers
        .get(COOKIE)?
        .to_str()
        .ok()?;
    header.split(';').find_map(|part| {
        let (key, value) = part.trim().split_once('=')?;
        (key == name).then(|| value.to_string())
    })
}

/// Branding for the admin shell: the sidebar header, the login card, and the
/// topbar below `md`, where the sidebar folds into a sheet.
#[derive(Debug, Clone)]
pub struct Brand {
    /// Display name (e.g. `"Acme"`).
    pub name: String,
    /// Optional logo URL (e.g. `"/logo.svg"`). Rendered as an `<img>` when present.
    pub logo: Option<String>,
}

impl Brand {
    /// Create a brand with the given name (surrounding whitespace is
    /// trimmed so `" Acme "` cannot break the `flex h-16` header).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into().trim().to_string(),
            logo: None,
        }
    }

    /// Attach a logo URL (blank values are ignored so an empty
    /// `logo("")` falls back to the name-only render instead of a
    /// broken-image icon).
    pub fn logo(mut self, logo: impl Into<String>) -> Self {
        let logo = logo.into().trim().to_string();
        if !logo.is_empty() {
            self.logo = Some(logo);
        }
        self
    }
}

/// The application-owned assets used by [`Panel::layout_shell`].
///
/// Tailwind's generated stylesheet is necessarily a call-site asset because
/// every application scans a different source tree. The Panel therefore takes
/// the generated stylesheet and the application's chosen font as values while
/// still owning the document markup that links them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ShellAssets {
    pub(crate) stylesheet: Asset,
    pub(crate) font: Font,
}

impl Panel {
    /// The top bar's tenant switcher: a disclosure listing the user's
    /// tenants, each a form that posts to the tenant-switch route. A user
    /// with fewer than two tenants has nothing to switch, and gets nothing.
    fn tenant_switcher<'a>(
        cx: &'a Cx,
        tenants: &[crate::tenancy::Membership],
        csrf: &str,
    ) -> BoxView<'a> {
        if tenants.len() < 2 {
            return ().boxed();
        }
        let current = crate::tenancy::membership(cx);
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
                        <button
                            type="submit"
                            aria-current=(selected.then_some("true"))
                            class="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-sm hover:bg-muted"
                        >
                            <span class="flex-1 truncate">(name)</span>
                            if selected {
                                icon(
                                    data: tablo_ui::icons::CHECK,
                                    attrs: attributes! { class="size-4" }
                                )
                            }
                        </button>
                    </form>
                }
                .boxed()
            })
            .collect();
        view! {
            cx =>
            <details class="relative" data-tenant-switcher="">
                <summary
                    class="flex cursor-pointer list-none items-center gap-1 rounded-md px-2 py-1 text-sm font-medium text-foreground hover:bg-muted [&::-webkit-details-marker]:hidden"
                >
                    <span class="max-w-40 truncate">(label)</span>
                    icon(
                        data: tablo_ui::icons::CHEVRON_DOWN,
                        attrs: attributes! { class="size-4 text-muted-foreground" }
                    )
                </summary>
                <div
                    class="absolute right-0 z-50 mt-1 min-w-48 rounded-md border border-border bg-popover p-1 text-popover-foreground shadow-md"
                >
                    for choice in choices {
                        (choice)
                    }
                </div>
            </details>
        }
        .boxed()
    }

    async fn theme_toggle(cx: &Cx) -> Result<BoxView<'_>> {
        use tablo_ui::{ButtonSize, ButtonVariant, button};

        // Both icons ship and the `dark` class on `<html>` picks the visible
        // one, so the control needs no script of its own to show the theme.
        Ok(view! {
            cx =>
            button(
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Icon,
                attrs: attributes! {
                    type="button"
                    aria-label="Toggle dark mode"
                    title="Toggle dark mode"
                    data-theme-toggle=""
                },
                icon(
                    data: tablo_ui::icons::SUN,
                    attrs: attributes! { class="dark:hidden" }
                )
                icon(
                    data: tablo_ui::icons::MOON,
                    attrs: attributes! { class="hidden dark:block" }
                )
            )
        }
        .boxed())
    }

    pub(crate) async fn render_brand(cx: &Cx) -> Result<BoxView<'_>> {
        let (name, logo) = if let Some(brand) = current(cx).and_then(|panel| panel.brand.as_ref()) {
            (brand.name.clone(), brand.logo.clone())
        } else {
            ("Tablo".to_string(), None)
        };
        // Without a logo the brand's initial stands in, on the primary token,
        // so the header keeps its mark.
        let mark: BoxView<'_> = match logo {
            Some(logo_url) => {
                let alt = name.clone();
                view! {
                    cx =>
                    <img
                        src=(logo_url)
                        alt=(alt)
                        width="28"
                        height="28"
                        class="size-7 shrink-0 rounded-md"
                    >
                }
                .boxed()
            }
            None => {
                let initial = name.chars().next().map(String::from).unwrap_or_default();
                view! {
                    cx =>
                    <span
                        aria-hidden="true"
                        class="flex size-7 shrink-0 items-center justify-center rounded-md bg-primary text-sm font-semibold text-primary-foreground"
                    >
                        (initial)
                    </span>
                }
                .boxed()
            }
        };
        Ok(view! {
            cx =>
            <div class="flex min-w-0 items-center gap-2 font-semibold text-foreground">
                (mark)
                <span class="truncate">(name)</span>
            </div>
        }
        .boxed())
    }

    async fn sidebar_navigation<'a>(
        cx: &'a Cx,
        nav_items: &[NavigationItem],
        current_path: &str,
        mobile_open: Signal<bool>,
    ) -> Result<BoxView<'a>> {
        use tablo_ui::{
            sidebar_group, sidebar_group_content, sidebar_group_label, sidebar_menu,
            sidebar_menu_button, sidebar_menu_item,
        };
        let mut nav_items = nav_items.to_vec();
        // Stable order: explicit `order` first, declaration order
        // breaking ties — a resource's `navigation()` override interleaves by
        // setting it.
        nav_items.sort_by_key(|item| item.order);
        // A Panel resolves every item it owns (`Panel::resource`, `page`,
        // `home`); one that
        // reaches the sidebar unresolved has no URL to render, which is a
        // framework bug rather than user error.
        debug_assert!(
            nav_items.iter().all(|item| item.url().is_some()),
            "navigation items are resolved by the Panel that owns them"
        );
        // One active entry: a home entry at the bare prefix matches every path
        // under it, so the longest matching URL wins, and the first such entry
        // among any that share it.
        let active = nav_items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.is_current_path(current_path))
            .min_by_key(|(_, item)| std::cmp::Reverse(item.url().map_or(0, str::len)))
            .map(|(index, _)| index);

        Ok(view! {
            cx =>
            sidebar_group(
                sidebar_group_label("Navigation")
                sidebar_group_content(
                    sidebar_menu(
                        for (index, item) in nav_items.iter().enumerate() {
                            let is_active = active == Some(index);
                            let attrs = sidebar_link(
                                cx,
                                attributes! {
                                    // Tapping a link in the mobile sheet closes
                                    // it; on desktop the navigation is the effect.
                                    @click=$(|_e: Event| mobile_open.set(false))
                                },
                            );
                            sidebar_menu_item(
                                sidebar_menu_button(
                                    active: is_active,
                                    href: item.url(),
                                    tooltip: Some(item.label.as_str()),
                                    attrs: attrs,
                                    if let Some(data) = item.icon.clone() {
                                        icon(data: data)
                                    }
                                    <span>(item.label.clone())</span>
                                )
                            )
                        }
                    )
                )
            )
        }
        .boxed())
    }

    /// Whether the persisted `sidebar_state` cookie asks for an expanded
    /// desktop sidebar (default: expanded).
    ///
    /// The value only seeds the runtime
    /// signal's initial `data-state`; after hydration the browser owns the
    /// state, and `assets/sidebar.js` mirrors changes back to the cookie.
    fn sidebar_starts_open(cx: &Cx) -> bool {
        request_cookie(cx, "sidebar_state").as_deref() != Some("collapsed")
    }

    /// Renders the shell around `slot` with an optional outer class.
    pub async fn render_shell<'a>(
        cx: &'a Cx,
        nav_items: &[NavigationItem],
        current_path: &str,
        slot: Child<'a>,
        extra_class: Option<String>,
    ) -> Result<BoxView<'a>> {
        // A hoisting body: the sidebar signals are declared while this view
        // resolves, and a page re-run resumes them from the client. The owned
        // copies pin the caller's navigation to the lazy body's lifetime.
        let nav_items = nav_items.to_vec();
        let current_path = current_path.to_string();
        Ok(Box::pin(HoistView::new(ThenView::new(async move {
            Self::render_shell_body(cx, &nav_items, &current_path, slot, extra_class).await
        }))))
    }

    async fn render_shell_body<'a>(
        cx: &'a Cx,
        nav_items: &[NavigationItem],
        current_path: &str,
        slot: Child<'a>,
        extra_class: Option<String>,
    ) -> Result<BoxView<'a>> {
        use tablo_ui::{
            SeparatorOrientation, SidebarCollapsible, separator, sidebar, sidebar_content,
            sidebar_header, sidebar_inset, sidebar_provider, sidebar_trigger,
        };

        let sidebar_open = signal(cx, || Self::sidebar_starts_open(cx));
        let mobile_open = signal(cx, || false);
        let outer_class = extra_class.clone().unwrap_or_default();
        let header_title = brand_name(cx);
        // One navigation tree: the upstream sidebar renders its children once
        // and shares them between the desktop panel and the mobile sheet.
        let navigation =
            Self::sidebar_navigation(cx, nav_items, current_path, mobile_open.clone()).await?;
        let sidebar_brand = Self::render_brand(cx).await?;
        let theme_toggle = Self::theme_toggle(cx).await?;
        // Signed-in identity + logout control, present only with a session
        // (ADR-0013). `ensure_token` runs before any streaming starts so the
        // logout form always carries a matching CSRF pair.
        let account_view: BoxView<'_> = match crate::auth::signed(cx) {
            Some(signed) => {
                let csrf = crate::csrf::ensure_token(cx);
                let logout = crate::auth::logout_url(cx);
                let name = signed.user.display_name().to_string();
                let initial = name.chars().next().map(String::from).unwrap_or_default();
                let switcher = Self::tenant_switcher(cx, signed.user.tenants(), &csrf);
                view! {
                    cx =>
                    <div class="flex items-center gap-2">
                        (switcher)
                        <span
                            aria-hidden="true"
                            class="flex size-7 shrink-0 items-center justify-center rounded-full bg-muted text-xs font-medium text-muted-foreground"
                        >
                            (initial)
                        </span>
                        <span class="max-sm:hidden text-sm font-medium text-foreground">
                            (name)
                        </span>
                        <form method="post" action=(logout)>
                            (crate::csrf::field(cx, &csrf))
                            tablo_ui::button(
                                variant: tablo_ui::ButtonVariant::Ghost,
                                size: tablo_ui::ButtonSize::Icon,
                                attrs: attributes! { type="submit" aria-label="Sign out" title="Sign out" },
                                icon(data: tablo_ui::icons::LOG_OUT)
                            )
                        </form>
                    </div>
                }
                .boxed()
            }
            None => view! { cx => <span></span> }.boxed(),
        };
        let notification_view: BoxView<'_> = match take_notification(cx) {
            Some(notification) => {
                crate::notification::render_notification(cx, notification, Default::default())
                    .await?
            }
            // No `<span>` placeholder: the toaster renders an `<ol>`,
            // which permits only `li`/`script`/`template` children — the empty
            // view renders nothing.
            None => ().boxed(),
        };
        // The page owns the live-toast signals; resolve the same handles here
        // (same helper, same request identity) and hand them to the shard.
        let LiveToast {
            status: toast_status,
            title: toast_title,
            description: toast_description,
            serial: toast_serial,
        } = live_toast(cx);

        Ok(view! {
            cx =>
            sidebar_provider(
                attrs: attributes! { class=(outer_class) },
                // `sidebar_rail` is intentionally not rendered: the header
                // `sidebar_trigger` is the explicit toggle, and the rail's
                // edge hit-area reads as stray chrome as a primary toggle.
                // Keep the component available (`tablo_ui::sidebar_rail`)
                // for opt-in `variant=inset`/`floating` layouts.
                sidebar(
                    open: $(sidebar_open.get()),
                    mobile_open: $(mobile_open.get()),
                    collapsible: SidebarCollapsible::Offcanvas,
                    sheet_attrs: attributes! {
                        id="mobile-sidebar-sheet"
                        aria-label="Navigation"
                        @keydown=$(|e: Event| {
                            if e.key == "Escape" {
                                mobile_open.set(false);
                            }
                        })
                        @click=$(|e: Event| {
                            if e.target.id == "mobile-sidebar-sheet" {
                                mobile_open.set(false);
                            }
                        })
                    },
                    sidebar_header(
                        <div class="flex items-center gap-2 px-2">
                            (sidebar_brand)
                            // Below `md` the sheet covers the inset header,
                            // so the sheet's own header carries the close
                            // control (upstream `examples/ui`); the mobile
                            // trigger in the inset header is behind the veil.
                            tablo_ui::button(
                                variant: tablo_ui::ButtonVariant::Ghost,
                                size: tablo_ui::ButtonSize::Icon,
                                attrs: attributes! {
                                    type="button"
                                    class="md:hidden ml-auto"
                                    aria-label="Close sidebar"
                                    @click=$(|_e: Event| mobile_open.set(false))
                                },
                                icon(data: tablo_ui::icons::X)
                            )
                        </div>
                    )
                    sidebar_content((navigation))
                )
                sidebar_inset(
                    sidebar_header(
                        // The desktop trigger collapses the rail; below md the
                        // mobile trigger opens the sheet instead (shadcn
                        // SidebarTrigger pair, upstream `examples/ui`).
                        sidebar_trigger(
                            open: $(sidebar_open.get()),
                            attrs: attributes! {
                                class="max-md:hidden"
                                aria-controls="mobile-sidebar-sheet"
                                @click=$(|_e: Event| sidebar_open.toggle())
                            }
                        )
                        sidebar_trigger(
                            open: $(mobile_open.get()),
                            attrs: attributes! {
                                class="md:hidden"
                                aria-controls="mobile-sidebar-sheet"
                                @click=$(|_e: Event| mobile_open.toggle())
                            }
                        )
                        // The sidebar carries the brand on desktop; below md
                        // it hides in the sheet, so the topbar names the
                        // panel there.
                        <div class="md:hidden font-semibold text-foreground">
                            (header_title)
                        </div>
                        // The separator stays a direct child of the topbar,
                        // where `sidebar_inset` sizes a vertical rule.
                        <div class="ml-auto">(theme_toggle)</div>
                        separator(orientation: SeparatorOrientation::Vertical)
                        (account_view)
                    )
                    // `sidebar_inset` is the document's one `<main>`; a second
                    // nested landmark is invalid and confuses landmark
                    // navigation (upstream `examples/ui` uses a plain div).
                    // The page container (`tablo_ui::page`) owns the width
                    // and the padding.
                    <div class="flex flex-1 flex-col">(slot)</div>
                )
                // Toast stack — the shadcn/Sonner surface, fixed bottom-right
                // and a polite live region so streamed swaps are announced.
                // `live_toaster` is the page-owned
                // in-place transport; the flash cookie's toast
                // rides beside it.
                tablo_ui::toaster(
                    (notification_view)
                    live_toaster(
                        status: $(toast_status),
                        title: $(toast_title),
                        description: $(toast_description),
                        serial: $(toast_serial)
                    )
                )
            ) // Scripts are owned by the document (layout_shell).
        }
        .boxed())
    }

    /// The panel shell around a page, as a complete HTML document: the layout
    /// every panel registers at its prefix unless [`Panel::layout`] replaces
    /// it, and what a replacement calls to keep the shell around its own
    /// markup.
    ///
    /// The stylesheet and font are supplied to the Panel builder with
    /// [`Self::shell_assets`]. A Panel without those values remains renderable
    /// for tests and custom document owners, but does not pretend that a CSS
    /// bundle exists. Errors from the page slot propagate unchanged when the
    /// document view is resolved.
    pub fn layout_shell<'a>(cx: &'a Cx, slot: Slot<'a>) -> BoxView<'a> {
        Box::pin(ThenView::new(Self::render_layout_shell(cx, slot)))
    }

    async fn render_layout_shell<'a>(cx: &'a Cx, slot: Slot<'a>) -> Result<BoxView<'a>> {
        use topcoat::router::request::uri;
        let path = uri(cx).path().to_string();
        // The panel's sidebar, or a single Home entry outside any panel.
        let nav_items = current(cx)
            .map(|panel| panel.nav_items.clone())
            .filter(|items| !items.is_empty())
            .unwrap_or_else(|| vec![NavigationItem::at("Home", super::gate::panel_prefix(cx))]);
        let shell = Self::render_shell(cx, &nav_items, &path, slot, None).await?;
        Self::render_document(cx, brand_name(cx), shell).await
    }

    /// Renders a complete HTML document around a page outside the panel prefix.
    pub async fn document<'a>(
        cx: &'a Cx,
        title: impl Into<String>,
        body: impl View + 'a,
    ) -> Result<BoxView<'a>> {
        Self::render_document(cx, title.into(), body.boxed()).await
    }

    /// Renders the HTML document with assets and the dark-mode class.
    pub(crate) async fn render_document<'a>(
        cx: &'a Cx,
        title: String,
        body: BoxView<'a>,
    ) -> Result<BoxView<'a>> {
        // The panel's default: the `<html class>` a first-time visitor gets,
        // and the fallback the blocking script uses when nothing is stored.
        let default_dark = current(cx).is_some_and(|panel| panel.dark_mode);
        let head: BoxView<'_> = match current(cx).and_then(|panel| panel.shell_assets) {
            Some(ShellAssets { stylesheet, font }) => view! {
                cx =>
                topcoat::dev::script()
                tablo_ui::theme_init_script(default_dark: default_dark)
                topcoat::runtime::script()
                topcoat::font::link(font: font)
                <link rel="stylesheet" href=(stylesheet)>
                <script src=(tablo_ui::SIDEBAR_JS) defer=""></script>
                <script src=(tablo_ui::THEME_JS) defer=""></script>
                <script src=(tablo_ui::DIALOG_JS) defer=""></script>
                <script src=(tablo_ui::WIRE_JS) defer=""></script>
                <script src=(tablo_ui::BULK_JS) defer=""></script>
                <script src=(tablo_ui::FILTERS_JS) defer=""></script>
                <script src=(tablo_ui::LIVE_SEARCH_JS) defer=""></script>
                <script src=(tablo_ui::SELECTS_JS) defer=""></script>
                <script src=(tablo_ui::VARIANT_JS) defer=""></script>
                <script src=(tablo_ui::NOTIFICATION_JS) defer=""></script>
                <script src=(tablo_ui::MUTATION_SUBMIT_JS) defer=""></script>
            }
            .boxed(),
            None => view! {
                cx =>
                topcoat::dev::script()
                tablo_ui::theme_init_script(default_dark: default_dark)
            }
            .boxed(),
        };
        let dark = match request_cookie(cx, "theme").as_deref() {
            Some("dark") => true,
            Some("light") => false,
            _ => default_dark,
        };
        let html_class = dark.then_some("dark");
        Ok(view! {
            cx =>
            <!DOCTYPE html>
            <html class=(html_class)>
                <head>
                    <title>(title)</title>
                    (head)
                </head>
                <body>(body)</body>
            </html>
        }
        .boxed())
    }
}

/// The request's panel brand name, `Tablo` without one.
fn brand_name(cx: &Cx) -> String {
    current(cx)
        .and_then(|panel| panel.brand.as_ref())
        .map_or_else(|| "Tablo".to_string(), |brand| brand.name.clone())
}

#[cfg(test)]
mod tests;
