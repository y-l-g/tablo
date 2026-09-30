//! Panel shell: document, sidebar, brand, theme, and notification view.
//!
//! Renders the Filament-grade shell framing every admin page. Depends on
//! `tablo-ui`, Topcoat's view and runtime, and the notification and
//! [`NavigationItem`] types — never on the [`Resource`] trait.

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

use super::Panel;
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

/// Branding for the admin shell (panel header + sidebar header).
#[derive(Debug, Clone)]
pub struct Brand {
    /// Display name (e.g. `"Acme"`).
    pub name: String,
    /// Optional logo URL (e.g. `"/logo.svg"`). Rendered as an `<img>` when present.
    pub logo: Option<String>,
}

impl Brand {
    /// Create a brand with the given name (GH #102: surrounding whitespace is
    /// trimmed so `" Acme "` cannot break the `flex h-16` header).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into().trim().to_string(),
            logo: None,
        }
    }

    /// Attach a logo URL (GH #102: blank values are ignored so an empty
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

/// Whether the shell starts in dark mode for a visitor with no stored choice.
/// Persisted via `theme.js` (`localStorage` + `theme` cookie).
///
/// Precedence: the stored preference wins in **both** directions over this
/// build-time default. The server renders `<html class>` from the `theme`
/// cookie when it carries `dark` or `light`, and from this default otherwise;
/// the blocking `theme_init_script` then applies `localStorage`, then the
/// cookie, with this default as its fallback. Rendering the cookie matters for
/// runtime navigation, which copies `<html>`'s attributes from the next page
/// and runs no script (GH #184, GH #395).
#[derive(Debug, Clone, Copy)]
pub struct DarkMode(pub bool);

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
    async fn theme_toggle(cx: &Cx) -> Result<BoxView<'_>> {
        use tablo_ui::{ButtonSize, ButtonVariant, button};

        Ok(view! {
            cx =>
            button(
                variant: ButtonVariant::Ghost,
                size: ButtonSize::Icon,
                attrs: attributes! { aria-label="Toggle dark mode" data-theme-toggle="" },
                <span aria-hidden="true">"◐"</span>
            )
        }
        .boxed())
    }

    pub(crate) async fn render_brand(cx: &Cx) -> Result<BoxView<'_>> {
        use topcoat::context::try_app_context;
        let (name, logo) = if let Some(brand) = try_app_context::<Brand>(cx) {
            (brand.name.clone(), brand.logo.clone())
        } else {
            ("Tablo".to_string(), None)
        };
        if let Some(logo_url) = logo {
            let alt = name.clone();
            Ok(view! {
                cx =>
                <div class="flex items-center gap-2 font-semibold text-foreground">
                    <img
                        src=(logo_url)
                        alt=(alt)
                        width="24"
                        height="24"
                        class="h-6 w-6 rounded"
                    >
                    (name)
                </div>
            }
            .boxed())
        } else {
            Ok(view! {
                cx =>
                <div class="flex items-center gap-2 font-semibold text-foreground">
                    (name)
                </div>
            }
            .boxed())
        }
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
        // A Panel resolves every item it owns (`Panel::resource`); one that
        // reaches the sidebar unresolved has no URL to render, which is a
        // framework bug rather than user error.
        debug_assert!(
            nav_items.iter().all(|item| item.url().is_some()),
            "navigation items are resolved by the Panel that owns them"
        );
        let current_path = current_path.to_string();

        Ok(view! {
            cx =>
            sidebar_group(
                sidebar_group_label("Navigation")
                sidebar_group_content(
                    sidebar_menu(
                        for item in &nav_items {
                            let is_active = item.is_current_path(&current_path);
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

    /// Render the Filament-grade Shell that frames every admin page.
    ///
    /// Composes Topcoat's upstream `sidebar` primitives (ADR-0007): the
    /// desktop panel and the mobile sheet share one navigation rendering, and
    /// the open state lives in runtime signals — `open` seeds from the
    /// `sidebar_state` cookie for the first paint, the triggers carry
    /// `@click` handlers, and `assets/sidebar.js` persists changes back to
    /// the cookie. Includes dark-mode toggle (Ghost button, persisted by
    /// `theme.js` to `localStorage` + the `theme` cookie) and the toast stack
    /// (shadcn/Sonner surface, fixed bottom-right). Additive `class` is
    /// allowed on the outer container only (narrow seam).
    ///
    /// Asset note: desktop persistence needs `assets/sidebar.js`
    /// (`tablo_ui::SIDEBAR_JS`), which [`Self::layout_shell`]'s document —
    /// not this function — emits (scripts are owned by the document). The
    /// mobile sheet (`#mobile-sidebar-sheet`) instead dismisses through its
    /// own runtime `@keydown`/`@click` handlers, so it needs no asset; the
    /// vendored `sheet`/`sidebar` primitives carry no note themselves
    /// (ADR-0007 sync guard). See ADR-0014.
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
            sidebar_footer, sidebar_header, sidebar_inset, sidebar_provider, sidebar_trigger,
        };

        let sidebar_open = signal(cx, || Self::sidebar_starts_open(cx));
        let mobile_open = signal(cx, || false);
        let outer_class = extra_class.clone().unwrap_or_default();
        let header_title = topcoat::context::try_app_context::<Brand>(cx)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| "Tablo".to_string());
        // One navigation tree: the upstream sidebar renders its children once
        // and shares them between the desktop panel and the mobile sheet.
        let navigation =
            Self::sidebar_navigation(cx, nav_items, current_path, mobile_open.clone()).await?;
        let sidebar_brand = Self::render_brand(cx).await?;
        let sidebar_theme_toggle = Self::theme_toggle(cx).await?;
        let header_theme_toggle = Self::theme_toggle(cx).await?;
        // Signed-in identity + logout control, present only with a session
        // (ADR-0013). `ensure_token` runs before any streaming starts so the
        // logout form always carries a matching CSRF pair.
        let account_view: BoxView<'_> = match crate::auth::current_user(cx) {
            Some(user) => {
                let csrf = crate::csrf::ensure_token(cx);
                let logout = crate::auth::logout_url(cx);
                view! {
                    cx =>
                    <div class="flex items-center gap-2">
                        <span class="text-sm text-muted-foreground">
                            (user.display_name)
                        </span>
                        <form method="post" action=(logout)>
                            (crate::csrf::field(cx, &csrf))
                            <button
                                type="submit"
                                class="text-sm text-muted-foreground underline"
                            >
                                "Sign out"
                            </button>
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
        // (same helper, same request identity) and hand them to the shard
        // (GH #154 §3).
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
                        <div class="flex items-center gap-2">
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
                    sidebar_footer((sidebar_theme_toggle))
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
                        separator(orientation: SeparatorOrientation::Vertical)
                        <div class="font-semibold text-foreground">(header_title)</div>
                        <div class="ml-auto flex items-center gap-2">
                            (account_view)
                            (header_theme_toggle)
                        </div>
                    )
                    // `sidebar_inset` is the document's one `<main>`; a second
                    // nested landmark is invalid and confuses landmark
                    // navigation (upstream `examples/ui` uses a plain div).
                    <div class="flex-1 mx-auto max-w-7xl w-full p-6">(slot)</div>
                )
                // Toast stack — the shadcn/Sonner surface, fixed bottom-right
                // and a polite live region so streamed swaps are announced.
                // `live_toaster` is the page-owned
                // in-place transport (GH #154 §3); the flash cookie's toast
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

    /// Convenience wrapper for `#[layout]` handlers: takes the layout's
    /// `slot: Slot<'_>` and renders the complete HTML document around the
    /// shell.
    ///
    /// The stylesheet and font are supplied to the Panel builder with
    /// [`Self::shell_assets`]. A Panel without those values remains renderable
    /// for tests and custom document owners, but does not pretend that a CSS
    /// bundle exists. Errors from the page slot propagate unchanged when the
    /// document view is resolved.
    pub async fn layout_shell<'a>(cx: &'a Cx, slot: Slot<'a>) -> Result<impl View + 'a> {
        use topcoat::{context::try_app_context, router::request::uri};
        let current = uri(cx).path().to_string();
        // Prefer declarative nav_items from Panel::resource, fallback to Home.
        let nav_items = try_app_context::<Vec<NavigationItem>>(cx)
            .cloned()
            .unwrap_or_else(|| vec![NavigationItem::at("Home", super::gate::panel_prefix(cx))]);
        let shell = Self::render_shell(cx, &nav_items, &current, slot, None).await?;
        let brand_title = try_app_context::<Brand>(cx)
            .map(|b| b.name.clone())
            .unwrap_or_else(|| "Tablo".to_string());
        Self::render_document(cx, brand_title, shell).await
    }

    /// The complete HTML document around a rendered body: assets, dark-mode
    /// class, and title. [`Self::layout_shell`] frames the panel shell with
    /// it; the standalone login page (ADR-0013) uses the same document so
    /// brand and dark mode carry over.
    ///
    /// The eleven shell scripts ship `defer`red (deliberate all-load policy,
    /// ADR-0014): parsing never waits for them, and every one is safe
    /// deferred — document-level listeners install after parse, and the
    /// `DOMContentLoaded` handlers still run, since deferred scripts execute
    /// first. The blocking `theme_init_script` stays inline so the `dark`
    /// class lands pre-paint.
    pub(crate) async fn render_document<'a>(
        cx: &'a Cx,
        title: String,
        body: BoxView<'a>,
    ) -> Result<BoxView<'a>> {
        use topcoat::context::try_app_context;
        // The build-time default: the `<html class>` a first-time visitor gets,
        // and the fallback the blocking script uses when nothing is stored.
        let default_dark = try_app_context::<DarkMode>(cx).is_some_and(|dm| dm.0);
        let head: BoxView<'_> = match try_app_context::<ShellAssets>(cx).copied() {
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

#[cfg(test)]
mod tests;
