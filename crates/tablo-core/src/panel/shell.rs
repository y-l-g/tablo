//! Renders the shell framing every admin page.

use topcoat::{
    Result,
    asset::Asset,
    context::Cx,
    font::Font,
    icon::icon,
    router::Slot,
    runtime::{Event, Signal, signal},
    view::{Attributes, BoxView, Child, ViewExt, attributes, internal::ThenView, view},
};

use super::{
    Panel,
    state::{ResourceTrail, current},
};
use crate::{navigation::NavigationItem, topcoat_compat::async_page};

/// `extra` plus the attribute that sends a sidebar link through runtime
/// navigation. The menu button writes the `href` itself, so this carries none.
fn sidebar_link(cx: &Cx, mut extra: Attributes) -> Attributes {
    let mut attrs = crate::navigation::runtime_link(cx, "");
    attrs.remove("href");
    extra.extend(attrs);
    extra
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
    async fn sidebar_navigation<'a>(
        cx: &'a Cx,
        nav_items: &[NavigationItem],
        current_path: &str,
        current_query: &str,
        mobile_open: Signal<bool>,
    ) -> Result<BoxView<'a>> {
        use tablo_ui::{
            sidebar_group, sidebar_group_content, sidebar_group_label, sidebar_menu,
            sidebar_menu_badge, sidebar_menu_button, sidebar_menu_item,
        };
        // A Panel resolves every item it owns (`Panel::resource`, `page`, `home`); one that
        // reaches the sidebar unresolved has no URL to render: a framework bug, not user error.
        debug_assert!(
            nav_items.iter().all(|item| item.url().is_some()),
            "navigation items are resolved by the Panel that owns them"
        );
        let nav_items = sidebar::in_sidebar_order(nav_items);
        // One active entry, chosen the way the breadcrumbs choose theirs.
        let active = crate::navigation::current_index(&nav_items, current_path, current_query);
        let sections = sidebar::sidebar_sections(nav_items);

        Ok(view! {
            cx =>
            for (label, entries) in sections {
                sidebar_group(
                    sidebar_group_label((label))
                    sidebar_group_content(
                        sidebar_menu(
                            for (index, item) in entries {
                                let is_active = active == Some(index);
                                let label = item.label.localize(cx);
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
                                        tooltip: Some(label.as_str()),
                                        attrs: attrs,
                                        if let Some(data) = item.icon.clone() {
                                            icon(data: data)
                                        }
                                        <span>(label.clone())</span>
                                    )
                                    if let Some(badge) = item.badge.clone() {
                                        sidebar_menu_badge((badge))
                                    }
                                )
                            }
                        )
                    )
                )
            }
        }
        .boxed())
    }

    /// Whether the persisted [`SIDEBAR_COOKIE`] asks for an expanded
    /// desktop sidebar (default: expanded).
    ///
    /// The value only seeds the runtime
    /// signal's initial `data-state`; after hydration the browser owns the
    /// state, and `assets/sidebar.js` mirrors changes back to the cookie.
    fn sidebar_starts_open(cx: &Cx) -> bool {
        request_cookie(cx, SIDEBAR_COOKIE).as_deref() != Some("collapsed")
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
        Ok(async_page(async move {
            Self::render_shell_body(cx, &nav_items, &current_path, slot, extra_class).await
        }))
    }

    async fn render_shell_body<'a>(
        cx: &'a Cx,
        nav_items: &[NavigationItem],
        current_path: &str,
        slot: Child<'a>,
        extra_class: Option<String>,
    ) -> Result<BoxView<'a>> {
        use tablo_ui::{
            SeparatorOrientation, SidebarCollapsible, SidebarVariant, separator, sidebar,
            sidebar_content, sidebar_footer, sidebar_header, sidebar_inset, sidebar_provider,
            sidebar_trigger,
        };

        let sidebar_open = signal(cx, || Self::sidebar_starts_open(cx));
        let mobile_open = signal(cx, || false);
        let outer_class = extra_class.clone().unwrap_or_default();
        let header_title = Self::brand_name(cx);
        // One navigation tree: the upstream sidebar renders its children once
        // and shares them between the desktop panel and the mobile sheet.
        // The query the browser sent: an entry that names parameters the request carries wins
        // over one that names none, at the same path.
        let current_query = topcoat::router::request::original_uri(cx)
            .query()
            .unwrap_or_default();
        let navigation = Self::sidebar_navigation(
            cx,
            nav_items,
            current_path,
            current_query,
            mobile_open.clone(),
        )
        .await?;
        let sidebar_brand = Self::render_brand(cx).await?;
        let resources: Vec<&ResourceTrail> = current(cx)
            .map(|panel| {
                panel
                    .nav_items
                    .iter()
                    .filter_map(|entry| entry.resource_list.as_ref())
                    .collect()
            })
            .unwrap_or_default();
        let trail = match super::error_page::current_error(cx) {
            Some(error) => {
                breadcrumbs::failed(nav_items, current_path, current_query, error.title())
            }
            None => breadcrumbs::trail(nav_items, &resources, current_path, current_query),
        };
        let crumbs = breadcrumbs::render(cx, trail);
        // The user and tenant menus, present only with a session (ADR-0013). `ensure_token` runs
        // before any streaming starts so their forms, and the language menu's, always carry a
        // matching CSRF pair.
        let (user_menu, tenant_menu): (BoxView<'_>, BoxView<'_>) = match crate::auth::signed(cx) {
            Some(signed) => {
                let csrf = crate::csrf::ensure_token(cx);
                let items: Vec<NavigationItem> = user_menu::account_items(cx)
                    .into_iter()
                    .chain(
                        current(cx)
                            .map(|panel| panel.user_menu.clone())
                            .unwrap_or_default(),
                    )
                    .collect();
                (
                    Self::user_menu(
                        cx,
                        signed.user.as_ref(),
                        &items,
                        crate::auth::logout_url(cx),
                        &csrf,
                    ),
                    Self::tenant_menu(cx, signed.user.tenants(), &csrf),
                )
            }
            // Without a session the footer still offers the theme, and the language where the
            // panel has one to pick: only that menu's forms need a token here.
            None => {
                let csrf = if crate::i18n::offered(cx).len() > 1 {
                    crate::csrf::ensure_token(cx)
                } else {
                    String::new()
                };
                (Self::theme_menu(cx, &csrf), ().boxed())
            }
        };
        let toaster = Self::toaster(cx).await?;

        Ok(view! {
            cx =>
            sidebar_provider(
                attrs: attributes! { class=(outer_class) },
                // No `sidebar_rail`: the header's `sidebar_trigger` is the one toggle.
                sidebar(
                    open: $(sidebar_open.get()),
                    mobile_open: $(mobile_open.get()),
                    collapsible: SidebarCollapsible::Icon,
                    variant: SidebarVariant::Inset,
                    sheet_attrs: attributes! {
                        id="mobile-sidebar-sheet"
                        aria-label=(crate::t!(cx, "tablo-shell-navigation"))
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
                        // Collapsed to icons, the rail leaves the brand only its mark: centred,
                        // it lines up with the menu icons below.
                        <div
                            class="flex items-center gap-2 px-2 md:group-data-[collapsible=icon]/sidebar:justify-center md:group-data-[collapsible=icon]/sidebar:px-0"
                        >
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
                                    aria-label=(crate::t!(cx, "tablo-shell-close-sidebar"))
                                    @click=$(|_e: Event| mobile_open.set(false))
                                },
                                icon(data: tablo_ui::icons::X)
                            )
                        </div>
                    )
                    sidebar_content(
                        // The icon rail is too narrow for a scrollbar: it still scrolls, without
                        // one.
                        attrs: attributes! {
                            class="md:group-data-[collapsible=icon]/sidebar:[scrollbar-width:none]"
                        },
                        (tenant_menu)
                        (navigation)
                    )
                    sidebar_footer((user_menu))
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
                        // The sidebar carries the brand on desktop; below md it hides in
                        // the sheet, so the topbar names the panel there.
                        <div
                            class="min-w-0 truncate font-semibold text-foreground md:hidden"
                        >
                            (header_title)
                        </div>
                        // The separator stays a direct child of the topbar,
                        // where `sidebar_inset` sizes a vertical rule.
                        separator(
                            orientation: SeparatorOrientation::Vertical,
                            attrs: attributes! { class="max-sm:hidden" }
                        )
                        (crumbs)
                        (super::search::topbar_end(cx))
                    )
                    // `sidebar_inset` is the document's one `<main>`; a second
                    // nested landmark is invalid and confuses landmark
                    // navigation (upstream `examples/ui` uses a plain div).
                    // The page container (`tablo_ui::page`) owns the width
                    // and the padding.
                    <div class="flex flex-1 flex-col">(slot)</div>
                )
                (toaster)
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
        // A page a creatable choice's dialog frames shows only itself and its toasts.
        if super::url::picking(cx) {
            let body = async_page(async move {
                Ok(view! {
                    cx =>
                    <main class="p-6">(slot)</main>
                    (Self::toaster(cx).await?)
                }
                .boxed())
            });
            let panel = current(cx).map(|panel| &**panel);
            return Self::render_document(cx, panel, Self::brand_name(cx), body).await;
        }
        // The browser's URL, not a rewrite's: the error page names where the visitor is.
        let path = topcoat::router::request::original_uri(cx)
            .path()
            .to_string();
        // The panel's sidebar, or a single Home entry outside any panel.
        let entries: Vec<_> = current(cx)
            .iter()
            .flat_map(|panel| &panel.nav_items)
            .filter(|entry| (entry.visible)(cx))
            .collect();
        // One count per badge, run together rather than one after another.
        let badges = futures_util::future::join_all(entries.iter().map(|entry| async {
            match &entry.badge {
                Some(badge) => Some(badge(cx).await),
                None => None,
            }
        }))
        .await;
        crate::notification::prefetch_bell(cx).await;
        let mut nav_items: Vec<NavigationItem> = entries
            .iter()
            .zip(badges)
            .map(|(entry, badge)| {
                let mut item = entry.item.clone();
                if let Some(badge) = badge {
                    item.badge = badge;
                }
                item
            })
            .collect();
        if nav_items.is_empty() {
            nav_items.push(NavigationItem::at(
                crate::t!(cx, "tablo-shell-home"),
                super::gate::panel_prefix(cx),
            ));
        }
        let shell = Self::render_shell(cx, &nav_items, &path, slot, None).await?;
        Self::render_document(
            cx,
            current(cx).map(|panel| &**panel),
            Self::brand_name(cx),
            shell,
        )
        .await
    }
}

mod brand;
mod breadcrumbs;
mod document;
mod language;
pub(super) mod sidebar;
mod theme;
mod toaster;
mod user_menu;

pub use brand::Brand;
pub(crate) use language::{LANGUAGE_SEGMENT, language_post};
use theme::{SIDEBAR_COOKIE, Theme};

use crate::cookie::request_cookie;
