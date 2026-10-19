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
    view::{Attributes, BoxView, Child, View, ViewExt, attributes, internal::ThenView, view},
};

use super::{Panel, state::current};
use crate::{
    navigation::NavigationItem,
    notification::{LiveToast, live_toast, live_toaster, take_notification},
    topcoat_compat::async_page,
};

/// `extra` plus the attribute that sends a sidebar link through runtime
/// navigation. The menu button writes the `href` itself, so this carries none.
fn sidebar_link(cx: &Cx, mut extra: Attributes) -> Attributes {
    let mut attrs = crate::navigation::runtime_link(cx, "");
    attrs.remove("href");
    extra.extend(attrs);
    extra
}

/// The cookie holding the visitor's theme, `light`, `dark` or `system`, which `assets/theme.js`
/// writes.
///
/// Prefixed because a cookie is shared by every app on a host whatever its port: another app's
/// `theme` on `localhost`, possibly HttpOnly and so beyond the toggle's reach, would otherwise
/// decide this panel's theme.
const THEME_COOKIE: &str = "tablo_theme";

/// The cookie holding the system's scheme, `light` or `dark`, which `assets/theme.js` mirrors
/// while the visitor's theme is `system`: the server cannot see the system's preference.
const SCHEME_COOKIE: &str = "tablo_scheme";

/// A visitor's theme choice, as [`THEME_COOKIE`] stores it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Theme {
    Light,
    Dark,
    /// The system's scheme, which [`SCHEME_COOKIE`] mirrors.
    System,
}

impl Theme {
    /// The choice the request's cookie stores, if any.
    fn stored(cx: &Cx) -> Option<Self> {
        match request_cookie(cx, THEME_COOKIE).as_deref() {
            Some("light") => Some(Self::Light),
            Some("dark") => Some(Self::Dark),
            Some("system") => Some(Self::System),
            _ => None,
        }
    }

    /// The choice the user menu shows as checked: the stored one, else the panel's default.
    fn current(cx: &Cx) -> Self {
        Self::stored(cx).unwrap_or(if Self::panel_default_dark(cx) {
            Self::Dark
        } else {
            Self::Light
        })
    }

    /// Whether the panel shows dark to a visitor who has not chosen.
    fn panel_default_dark(cx: &Cx) -> bool {
        current(cx).is_some_and(|panel| panel.dark_mode)
    }

    /// Whether the request's page renders dark: the stored choice, the mirrored system scheme
    /// under `system`, else the panel's default.
    fn renders_dark(cx: &Cx) -> bool {
        match Self::stored(cx) {
            Some(Self::Dark) => true,
            Some(Self::Light) => false,
            Some(Self::System) => match request_cookie(cx, SCHEME_COOKIE).as_deref() {
                Some("dark") => true,
                Some("light") => false,
                _ => Self::panel_default_dark(cx),
            },
            None => Self::panel_default_dark(cx),
        }
    }

    /// The name the cookie and the theme script use.
    fn name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }
}

/// The cookie holding the desktop sidebar's state, which `assets/sidebar.js` writes.
const SIDEBAR_COOKIE: &str = "tablo_sidebar";

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

/// Branding for the panel's shell: the sidebar header, the login card, and the
/// topbar below `md`, where the sidebar folds into a sheet.
#[derive(Debug, Clone)]
pub struct Brand {
    /// Display name (e.g. `"Acme"`).
    pub name: String,
    /// Optional logo URL (e.g. `"/logo.svg"`). Rendered as an `<img>` when present.
    pub logo: Option<String>,
    /// Optional favicon URL (e.g. `"/favicon.svg"`), the `<link rel="icon">` of every page the
    /// panel renders, its sign-in pages included.
    pub favicon: Option<String>,
}

impl Brand {
    /// Create a brand with the given name (surrounding whitespace is
    /// trimmed so `" Acme "` cannot break the `flex h-16` header).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into().trim().to_string(),
            logo: None,
            favicon: None,
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

    /// Attach a favicon URL; a blank value is ignored, leaving the browser's default.
    pub fn favicon(mut self, favicon: impl Into<String>) -> Self {
        let favicon = favicon.into().trim().to_string();
        if !favicon.is_empty() {
            self.favicon = Some(favicon);
        }
        self
    }
}

/// Where a brand mark renders, which sets its size.
#[derive(Clone, Copy)]
enum BrandSize {
    /// The sidebar header and the narrow topbar: 28px.
    Sidebar,
    /// Above the card of a page outside the shell, such as sign-in: 40px.
    Page,
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
    /// The panel's brand: its logo, or its initial on the primary color, and its name.
    fn current_brand(cx: &Cx) -> (String, Option<String>) {
        match current(cx).and_then(|panel| panel.brand.as_ref()) {
            Some(brand) => (brand.name.clone(), brand.logo.clone()),
            None => ("Tablo".to_string(), None),
        }
    }

    /// The request's panel brand name, `Tablo` without one.
    pub(crate) fn brand_name(cx: &Cx) -> String {
        Self::current_brand(cx).0
    }

    /// The brand's mark at `size`: its logo, or without one its initial on the primary token, so
    /// a header keeps its mark.
    fn brand_mark<'a>(
        cx: &'a Cx,
        name: &str,
        logo: Option<String>,
        size: BrandSize,
    ) -> BoxView<'a> {
        // A page logo keeps its own width at the page mark's height.
        let (width, height, logo_class, initial_class) = match size {
            BrandSize::Sidebar => (
                Some("28"),
                "28",
                "size-7 shrink-0 rounded-md",
                "flex size-7 shrink-0 items-center justify-center rounded-md bg-primary text-sm \
                 font-semibold text-primary-foreground",
            ),
            BrandSize::Page => (
                None,
                "40",
                "h-10 w-auto shrink-0",
                "flex size-10 shrink-0 items-center justify-center rounded-lg bg-primary text-lg \
                 font-semibold text-primary-foreground",
            ),
        };
        match logo {
            Some(logo_url) => {
                // A page always shows the name beside its logo, so the logo is decoration there;
                // the icon rail hides the sidebar's name, so the sidebar logo names the brand.
                let alt = match size {
                    BrandSize::Sidebar => name.to_string(),
                    BrandSize::Page => String::new(),
                };
                view! {
                    cx =>
                    <img
                        src=(logo_url)
                        alt=(alt)
                        width=(width)
                        height=(height)
                        class=(logo_class)
                    >
                }
                .boxed()
            }
            None => {
                let initial = name.chars().next().map(String::from).unwrap_or_default();
                view! {
                    cx =>
                    <span aria-hidden="true" class=(initial_class)>(initial)</span>
                }
                .boxed()
            }
        }
    }

    /// The brand as the sidebar header shows it: the mark beside the name.
    pub(crate) async fn render_brand(cx: &Cx) -> Result<BoxView<'_>> {
        let (name, logo) = Self::current_brand(cx);
        let mark = Self::brand_mark(cx, &name, logo, BrandSize::Sidebar);
        Ok(view! {
            cx =>
            <div class="flex min-w-0 items-center gap-2 font-semibold text-foreground">
                (mark)
                <span class="truncate md:group-data-[collapsible=icon]/sidebar:hidden">
                    (name)
                </span>
            </div>
        }
        .boxed())
    }

    /// The brand as a page outside the shell shows it, above its card: the mark over the name.
    pub(crate) fn render_page_brand(cx: &Cx) -> BoxView<'_> {
        let (name, logo) = Self::current_brand(cx);
        let mark = Self::brand_mark(cx, &name, logo, BrandSize::Page);
        view! {
            cx =>
            <div class="flex flex-col items-center gap-3 text-center" data-brand="">
                (mark)
                <span class="text-lg font-semibold tracking-tight text-foreground">
                    (name)
                </span>
            </div>
        }
        .boxed()
    }

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
        // A Panel resolves every item it owns (`Panel::resource`, `page`,
        // `home`); one that
        // reaches the sidebar unresolved has no URL to render, which is a
        // framework bug rather than user error.
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
            SeparatorOrientation, SidebarCollapsible, separator, sidebar, sidebar_content,
            sidebar_footer, sidebar_header, sidebar_inset, sidebar_provider, sidebar_trigger,
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
        let resource_lists: Vec<&str> = current(cx)
            .map(|panel| {
                panel
                    .nav_items
                    .iter()
                    .filter_map(|entry| entry.resource_list.as_deref())
                    .collect()
            })
            .unwrap_or_default();
        let trail = match super::error_page::current_error(cx) {
            Some(error) => {
                breadcrumbs::failed(nav_items, current_path, current_query, error.title())
            }
            None => breadcrumbs::trail(nav_items, &resource_lists, current_path, current_query),
        };
        let crumbs = breadcrumbs::render(cx, trail);
        // The user and tenant menus, present only with a session (ADR-0013). `ensure_token` runs
        // before any streaming starts so their forms always carry a matching CSRF pair.
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
            // Without a session the footer still offers the theme.
            None => (Self::theme_menu(cx), ().boxed()),
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
                    collapsible: SidebarCollapsible::Icon,
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
                                    aria-label="Close sidebar"
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
                        // The sidebar carries the brand on desktop; below md
                        // it hides in the sheet, so the topbar names the
                        // panel there.
                        <div class="md:hidden font-semibold text-foreground">
                            (header_title)
                        </div>
                        // The separator stays a direct child of the topbar,
                        // where `sidebar_inset` sizes a vertical rule.
                        separator(
                            orientation: SeparatorOrientation::Vertical,
                            attrs: attributes! { class="max-sm:hidden" }
                        )
                        (crumbs)
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
            nav_items.push(NavigationItem::at("Home", super::gate::panel_prefix(cx)));
        }
        let shell = Self::render_shell(cx, &nav_items, &path, slot, None).await?;
        Self::render_document(cx, Self::brand_name(cx), shell).await
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
        let head: BoxView<'_> = match current(cx).and_then(|panel| panel.shell_assets) {
            Some(ShellAssets { stylesheet, font }) => view! {
                cx =>
                topcoat::dev::script()
                topcoat::runtime::script()
                topcoat::font::link(font: font)
                <link rel="stylesheet" href=(stylesheet)>
                // Modules, so each script's top-level names stay its own: as classic scripts they
                // share one global scope, where a later script's `partsOf` replaces an earlier one's.
                // A module is deferred like these were.
                for script in tablo_ui::SHELL_SCRIPTS {
                    <script src=(script) type="module"></script>
                }
            }
            .boxed(),
            None => view! { cx => topcoat::dev::script() }.boxed(),
        };
        // The server renders the stored choice on every page: runtime navigation copies
        // `<html>`'s attributes from the next page and runs no script.
        let html_class = Theme::renders_dark(cx).then_some("dark");
        let favicon = current(cx)
            .and_then(|panel| panel.brand.as_ref())
            .and_then(|brand| brand.favicon.clone());
        Ok(view! {
            cx =>
            <!DOCTYPE html>
            <html class=(html_class)>
                <head>
                    <meta charset="utf-8">
                    // Without it a phone lays the page out 980px wide and zooms it out.
                    <meta name="viewport" content="width=device-width, initial-scale=1">
                    <title>(title)</title>
                    if let Some(favicon) = favicon {
                        <link rel="icon" href=(favicon)>
                    }
                    (head)
                </head>
                <body>(body)</body>
            </html>
        }
        .boxed())
    }
}

mod breadcrumbs;
mod sidebar;
mod user_menu;
