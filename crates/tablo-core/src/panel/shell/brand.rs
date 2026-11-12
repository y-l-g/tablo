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
    /// The primary color, light then dark, and the text on it.
    palette: Palette,
    radius: Option<Radius>,
    theme: Theme,
    sidebar: Option<SidebarFrame>,
}

/// A [`Brand`]'s colors over its theme's primary tokens.
#[derive(Debug, Clone, Default)]
struct Palette {
    color: Option<String>,
    dark_color: Option<String>,
    on_color: Option<String>,
}

/// One of the looks Tablo ships, which a [`Brand`] picks with [`Brand::theme`]: its palette in both
/// color schemes, its corners, its shadows, and the [`SidebarFrame`] it suggests.
///
/// A brand's [`color`](Brand::color), [`radius`](Brand::radius) and
/// [`sidebar`](Brand::sidebar), and the tokens an app's stylesheet redeclares, apply over the
/// theme.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Theme {
    /// Tablo's own: a jade primary on cool neutrals, the page inset in the sidebar's frame.
    #[default]
    Jade,
    /// An amber primary on plain grays, a white sidebar ruled off from the page, and rounder
    /// cards: close to Filament's default look.
    Amber,
    /// Black and white with square corners, beside a charcoal sidebar in either color scheme.
    Graphite,
    /// A violet primary on tinted grays, rounder still, with the sidebar floating beside the page.
    Iris,
}

impl Theme {
    /// The `data-tablo-theme` the stylesheet selects the theme's tokens by; the default theme's
    /// are the stylesheet's `:root` ones, so it needs none.
    pub(crate) const fn attribute(self) -> Option<&'static str> {
        match self {
            Self::Jade => None,
            Self::Amber => Some("amber"),
            Self::Graphite => Some("graphite"),
            Self::Iris => Some("iris"),
        }
    }

    /// The frame the theme was drawn beside, unless the brand picks another.
    pub(super) const fn sidebar(self) -> SidebarFrame {
        match self {
            Self::Jade => SidebarFrame::Inset,
            Self::Amber | Self::Graphite => SidebarFrame::Ruled,
            Self::Iris => SidebarFrame::Floating,
        }
    }
}

/// How the sidebar sits beside the page, which a [`Brand`] picks with [`Brand::sidebar`] over its
/// [`Theme`]'s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SidebarFrame {
    /// Flush with the window's edge, a rule between it and the page: Amber's and Graphite's.
    Ruled,
    /// The page a sheet inset in the sidebar's color, the sidebar its frame: Jade's.
    Inset,
    /// A card floating beside the page, clear of the window's edge: Iris's.
    Floating,
}

impl SidebarFrame {
    pub(crate) const fn variant(self) -> tablo_ui::SidebarVariant {
        match self {
            Self::Ruled => tablo_ui::SidebarVariant::Sidebar,
            Self::Inset => tablo_ui::SidebarVariant::Inset,
            Self::Floating => tablo_ui::SidebarVariant::Floating,
        }
    }
}

/// How round a [`Brand`]'s controls, cards and badges are, over its theme's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Radius {
    /// Square corners.
    None,
    /// `0.25rem`.
    Small,
    /// `0.5rem`, Jade's.
    Medium,
    /// `0.75rem`.
    Large,
    /// `1rem`.
    ExtraLarge,
}

impl Radius {
    const fn css(self) -> &'static str {
        match self {
            Self::None => "0rem",
            Self::Small => "0.25rem",
            Self::Medium => "0.5rem",
            Self::Large => "0.75rem",
            Self::ExtraLarge => "1rem",
        }
    }
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
            palette: Palette::default(),
            radius: None,
            theme: Theme::default(),
            sidebar: None,
        }
    }

    /// Paint the panel's primary color, its buttons, links, focus rings and brand mark, in
    /// `color`: a hex color or a color function such as `oklch(0.55 0.2 260)`. It paints the dark
    /// theme too, unless [`dark_color`](Self::dark_color) says otherwise. Mounting the panel
    /// refuses any other spelling, which the page's stylesheet would carry as it is.
    pub fn color(mut self, color: impl Into<String>) -> Self {
        self.palette.color = Some(color.into().trim().to_string());
        self
    }

    /// Paint the primary color in `color` while the page renders dark, such as a lighter shade
    /// of [`color`](Self::color) for a dark background.
    pub fn dark_color(mut self, color: impl Into<String>) -> Self {
        self.palette.dark_color = Some(color.into().trim().to_string());
        self
    }

    /// Paint the text on the primary color in `color`, over the theme's: a dark
    /// color for a light [`color`](Self::color), such as yellow.
    pub fn on_color(mut self, color: impl Into<String>) -> Self {
        self.palette.on_color = Some(color.into().trim().to_string());
        self
    }

    /// Dress the panel in `theme` instead of [`Theme::Jade`]: its pages, sign-in included.
    pub fn theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    /// Frame the page with the sidebar as `frame` says instead of as the theme does.
    pub fn sidebar(mut self, frame: SidebarFrame) -> Self {
        self.sidebar = Some(frame);
        self
    }

    /// The theme the brand picks.
    pub(crate) fn current_theme(&self) -> Theme {
        self.theme
    }

    /// The sidebar's frame: the brand's, else its theme's.
    pub(crate) fn sidebar_frame(&self) -> SidebarFrame {
        self.sidebar.unwrap_or_else(|| self.theme.sidebar())
    }

    /// Round the panel's corners to `radius`.
    pub fn radius(mut self, radius: Radius) -> Self {
        self.radius = Some(radius);
        self
    }

    /// The colors mounting refuses: each that is no CSS color.
    pub(crate) fn declaration_errors(&self) -> Vec<crate::DeclarationErrorKind> {
        let Palette {
            color,
            dark_color,
            on_color,
        } = &self.palette;
        [color, dark_color, on_color]
            .into_iter()
            .flatten()
            .filter(|value| !crate::column::is_css_color(value))
            .map(|value| crate::DeclarationErrorKind::BrandColor {
                value: value.clone(),
            })
            .collect()
    }

    /// The style sheet that lays the brand's colors and radius over its theme; `None`
    /// when it sets neither, or a color mounting would refuse.
    pub(crate) fn style(&self) -> Option<String> {
        if !self.declaration_errors().is_empty() {
            return None;
        }
        let Palette {
            color,
            dark_color,
            on_color,
        } = &self.palette;
        let tokens = |primary: Option<&String>| {
            let mut tokens = String::new();
            if let Some(primary) = primary {
                tokens.push_str(&format!("--primary:{primary};--ring:{primary};"));
            }
            if let Some(on) = on_color {
                tokens.push_str(&format!("--primary-foreground:{on};"));
            }
            tokens
        };
        let mut light = tokens(color.as_ref());
        if let Some(radius) = self.radius {
            light.push_str(&format!("--radius:{};", radius.css()));
        }
        let dark = tokens(dark_color.as_ref().or(color.as_ref()));
        let mut style = String::new();
        if !light.is_empty() {
            style.push_str(&format!(":root{{{light}}}"));
        }
        if !dark.is_empty() {
            style.push_str(&format!(":root.dark{{{dark}}}"));
        }
        (!style.is_empty()).then_some(style)
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
                view! { <span aria-hidden="true" class=(initial_class)>(initial)</span> }.boxed()
            }
        }
    }

    /// The brand as the sidebar header shows it: the mark beside the name.
    pub(crate) async fn render_brand(cx: &Cx) -> Result<BoxView<'_>> {
        let (name, logo) = Self::current_brand(cx);
        let mark = Self::brand_mark(cx, &name, logo, BrandSize::Sidebar);
        Ok(view! {
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
