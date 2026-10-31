//! The panel's brand: its name and logos, in the sidebar header and above the sign-in card.

use topcoat::{
    Result,
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use super::{super::Panel, current};

/// Branding for the panel's shell: the sidebar header, the login card, and the
/// topbar below `md`, where the sidebar folds into a sheet.
#[derive(Debug, Clone)]
pub struct Brand {
    /// Display name (e.g. `"Acme"`).
    pub name: String,
    /// Optional logo URL (e.g. `"/logo.svg"`). Rendered as an `<img>` when present.
    pub logo: Option<String>,
    /// Optional logo URL shown in place of [`logo`](Self::logo) while the page renders dark.
    pub dark_logo: Option<String>,
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
            dark_logo: None,
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

    /// Attach the logo URL shown in place of [`logo`](Self::logo) while the page renders dark,
    /// such as a light wordmark for a dark background; a blank value is ignored. Without a
    /// [`logo`](Self::logo) it is ignored too, and the brand shows its initial in either theme.
    pub fn dark_logo(mut self, logo: impl Into<String>) -> Self {
        let logo = logo.into().trim().to_string();
        if !logo.is_empty() {
            self.dark_logo = Some(logo);
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

/// A brand's logo URLs: the one it shows, and the one it shows instead while the page renders
/// dark.
struct Logos {
    light: String,
    dark: Option<String>,
}

/// Where a brand mark renders, which sets its size.
#[derive(Clone, Copy)]
enum BrandSize {
    /// The sidebar header and the narrow topbar: 28px.
    Sidebar,
    /// Above the card of a page outside the shell, such as sign-in: 40px.
    Page,
}

impl Panel {
    /// The panel's brand: its name and its logos, light then dark, or none for its initial on
    /// the primary color.
    fn current_brand(cx: &Cx) -> (String, Option<Logos>) {
        match current(cx).and_then(|panel| panel.brand.as_ref()) {
            Some(brand) => (
                brand.name.clone(),
                brand.logo.clone().map(|light| Logos {
                    light,
                    dark: brand.dark_logo.clone(),
                }),
            ),
            None => ("Tablo".to_string(), None),
        }
    }

    /// The request's panel brand name, `Tablo` without one.
    pub(crate) fn brand_name(cx: &Cx) -> String {
        Self::current_brand(cx).0
    }

    /// The brand's mark at `size`: its logo, or without one its initial on the primary token, so
    /// a header keeps its mark.
    fn brand_mark<'a>(cx: &'a Cx, name: &str, logo: Option<Logos>, size: BrandSize) -> BoxView<'a> {
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
            Some(Logos { light, dark }) => {
                // A page always shows the name beside its logo, so the logo is decoration there;
                // the icon rail hides the sidebar's name, so the sidebar logo names the brand.
                let alt = match size {
                    BrandSize::Sidebar => name.to_string(),
                    BrandSize::Page => String::new(),
                };
                // Both images render and the `dark` class picks one, so the theme toggle swaps
                // the logo without a request.
                let light_class = match dark {
                    Some(_) => format!("{logo_class} dark:hidden"),
                    None => logo_class.to_string(),
                };
                let dark_class = format!("{logo_class} hidden dark:block");
                view! {
                    cx =>
                    <img
                        src=(light)
                        alt=(alt.clone())
                        width=(width)
                        height=(height)
                        class=(light_class)
                    >
                    if let Some(dark_url) = dark {
                        <img
                            src=(dark_url)
                            alt=(alt)
                            width=(width)
                            height=(height)
                            class=(dark_class)
                        >
                    }
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
}
