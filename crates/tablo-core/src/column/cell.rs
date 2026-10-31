//! The view of a text cell, which [`TextColumn`](super::TextColumn) and
//! [`ComputedColumn`](super::ComputedColumn) share: its badge, color, icon, link, tooltip,
//! truncation, copy button and placeholder. Free functions over owned data, so tablo-core
//! compiles the view once rather than once for each model and field type.

use topcoat::{
    context::Cx,
    icon::{IconData, icon},
    runtime::Event,
    view::*,
};

use crate::{
    Text,
    display::{Color, FontWeight},
};

/// The schemes a cell's link may carry; a relative URL carries none.
const LINK_SCHEMES: &[&str] = &["http", "https", "mailto", "tel"];

/// `href` when a cell may link to it: a relative URL, or an absolute one whose scheme is one of
/// [`LINK_SCHEMES`]. The check parses as a browser does, so a `javascript:` URL hidden behind
/// whitespace, a tab or an uppercase letter is refused like a plain one.
pub(crate) fn link_href(href: &str) -> Option<&str> {
    match url::Url::parse(href) {
        Ok(url) => LINK_SCHEMES.contains(&url.scheme()).then_some(href),
        Err(url::ParseError::RelativeUrlWithoutBase) => Some(href),
        Err(_) => None,
    }
}

/// `text` cut to its first `chars` characters followed by an ellipsis, or `None` when it holds no
/// more than `chars`.
pub(crate) fn truncate(text: &str, chars: usize) -> Option<String> {
    let (cut, _) = text.char_indices().nth(chars)?;
    Some(format!("{}…", text[..cut].trim_end()))
}

/// What one text cell shows.
#[derive(Default)]
pub(crate) struct TextCell {
    pub(crate) text: String,
    /// The value's color: a badge's dot, or the text's when it is no badge.
    pub(crate) color: Option<Color>,
    pub(crate) badge: bool,
    /// Whether the digits take equal widths, so a column of amounts lines up.
    pub(crate) tabular: bool,
    pub(crate) weight: FontWeight,
    /// The icon before the value, in its own color.
    pub(crate) icon: Option<(IconData, Option<Color>)>,
    pub(crate) description: Option<String>,
    /// Where the value links, before [`link_href`] checks it.
    pub(crate) url: Option<String>,
    pub(crate) new_tab: bool,
    pub(crate) tooltip: Option<String>,
    /// The most characters the value shows.
    pub(crate) limit: Option<usize>,
    pub(crate) copyable: bool,
    /// What an empty value shows in place of the dash.
    pub(crate) placeholder: Option<Text>,
}

/// Render `cell`.
pub(crate) fn text_cell(cx: &Cx, cell: TextCell) -> BoxView<'_> {
    let value = if cell.text.trim().is_empty() {
        let placeholder = cell
            .placeholder
            .map_or_else(|| "—".to_string(), |placeholder| placeholder.localize(cx));
        view! { cx => <span class="text-muted-foreground">(placeholder)</span> }.boxed()
    } else {
        value_view(cx, &cell)
    };
    match cell
        .description
        .filter(|description| !description.trim().is_empty())
    {
        None => value,
        Some(description) => view! {
            cx =>
            <div class="flex min-w-0 flex-col">
                <span class="truncate">(value)</span>
                <span class="truncate text-xs text-muted-foreground">
                    (description)
                </span>
            </div>
        }
        .boxed(),
    }
}

/// Render `cells`, one badge each, wrapping onto as many lines as they need; none renders the
/// dash an empty text cell does.
pub(crate) fn badges_cell(cx: &Cx, cells: Vec<TextCell>) -> BoxView<'_> {
    if cells.is_empty() {
        return text_cell(cx, TextCell::default());
    }
    let badges: Vec<BoxView<'_>> = cells.iter().map(|cell| value_view(cx, cell)).collect();
    view! {
        cx =>
        <span class="flex flex-wrap gap-1">
            for badge in badges {
                (badge)
            }
        </span>
    }
    .boxed()
}

/// A non-empty value: its badge or text, linked, titled and followed by its copy button as `cell`
/// says.
fn value_view<'a>(cx: &'a Cx, cell: &TextCell) -> BoxView<'a> {
    let truncated = cell.limit.and_then(|limit| truncate(&cell.text, limit));
    let title = cell
        .tooltip
        .clone()
        .filter(|tooltip| !tooltip.trim().is_empty())
        .or_else(|| truncated.is_some().then(|| cell.text.clone()));
    let shown = truncated.unwrap_or_else(|| cell.text.clone());
    let glyph = cell
        .icon
        .clone()
        .map(|(data, color)| icon_view(cx, data, color));
    let content = if cell.badge {
        badge_view(cx, shown, cell.color, glyph)
    } else {
        let class = [
            Some("inline-flex items-center gap-1.5"),
            cell.color.map(Color::text_class),
            cell.tabular.then_some("tabular-nums"),
            Some(cell.weight.class()).filter(|class| !class.is_empty()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        view! {
            cx =>
            <span class=(class)>
                if let Some(glyph) = glyph {
                    (glyph)
                }
                (shown)
            </span>
        }
        .boxed()
    };
    let content = match cell.url.as_deref().and_then(link_href) {
        Some(href) => {
            let href = href.to_string();
            let (target, rel) = if cell.new_tab {
                (Some("_blank"), Some("noopener noreferrer"))
            } else {
                (None, None)
            };
            view! {
                cx =>
                <a
                    href=(href)
                    target=(target)
                    rel=(rel)
                    class="underline-offset-4 hover:underline"
                    data-cell-link=""
                >
                    (content)
                </a>
            }
            .boxed()
        }
        None => content,
    };
    let copy = cell.copyable.then(|| copy_button(cx, cell.text.clone()));
    if title.is_none() && copy.is_none() {
        return content;
    }
    view! {
        cx =>
        <span class="inline-flex max-w-full items-center gap-1" title=(title)>
            (content)
            if let Some(copy) = copy {
                (copy)
            }
        </span>
    }
    .boxed()
}

/// `data` at a cell's icon size, in `color` when it has one.
pub(crate) fn icon_view(cx: &Cx, data: IconData, color: Option<Color>) -> BoxView<'_> {
    let class = match color {
        Some(color) => format!("size-4 shrink-0 {}", color.text_class()),
        None => "size-4 shrink-0".to_string(),
    };
    view! {
        cx =>
        icon(data: data, attrs: attributes! { class=(class) aria-hidden="true" })
    }
    .boxed()
}

/// `text` as an outline badge, led by its icon, or else by a dot in `color` when it has one.
fn badge_view<'a>(
    cx: &'a Cx,
    text: String,
    color: Option<Color>,
    glyph: Option<BoxView<'a>>,
) -> BoxView<'a> {
    view! {
        cx =>
        tablo_ui::badge(
            variant: tablo_ui::BadgeVariant::Outline,
            if let Some(glyph) = glyph {
                (glyph)
            } else if let Some(color) = color {
                <span class=(color.dot_class()) aria-hidden="true"></span>
            }
            (text)
        )
    }
    .boxed()
}

/// A button copying `value` to the clipboard.
fn copy_button(cx: &Cx, value: String) -> BoxView<'_> {
    let label = crate::t!(cx, "tablo-list-copy");
    let copied = value.clone();
    view! {
        cx =>
        <button
            type="button"
            class="inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-accent hover:text-foreground"
            aria-label=(label.clone())
            title=(label)
            data-copy=(value)
            @click=$(|_e: Event| {
                raw!("navigator.clipboard?.writeText(String(${copied}))", ());
            })
        >
            icon(
                data: tablo_ui::icons::COPY,
                attrs: attributes! { class="size-3.5" aria-hidden="true" }
            )
        </button>
    }
    .boxed()
}

#[cfg(test)]
mod tests;
