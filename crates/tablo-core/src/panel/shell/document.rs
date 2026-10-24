//! The HTML document around a page: head, assets, favicon and theme.

use topcoat::{
    Result,
    context::Cx,
    view::{BoxView, View, ViewExt, view},
};

use super::{ShellAssets, Theme, current};
use crate::panel::{Panel, state::PanelState};

impl Panel {
    /// Renders a complete HTML document around a page, with the shell settings of the panel the
    /// request is under. A page outside every prefix belongs to no panel, so it renders without
    /// them; such a page renders through [`PanelHandle::document`](crate::PanelHandle::document).
    pub async fn document<'a>(
        cx: &'a Cx,
        title: impl Into<String>,
        body: impl View + 'a,
    ) -> Result<BoxView<'a>> {
        Self::render_document(
            cx,
            current(cx).map(|panel| &**panel),
            title.into(),
            body.boxed(),
        )
        .await
    }

    /// Renders the HTML document with `panel`'s assets, favicon and dark-mode default.
    pub(crate) async fn render_document<'a>(
        cx: &'a Cx,
        panel: Option<&PanelState>,
        title: String,
        body: BoxView<'a>,
    ) -> Result<BoxView<'a>> {
        let head: BoxView<'_> = match panel.and_then(|panel| panel.shell_assets) {
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
                // Where `rich_text.js` imports the editor from, once a page holds a rich text
                // control: no page loads it before.
                <meta name="tablo-tiptap" content=(tablo_ui::TIPTAP_JS)>
            }
            .boxed(),
            None => view! { cx => topcoat::dev::script() }.boxed(),
        };
        // The server renders the stored choice on every page: runtime navigation copies
        // `<html>`'s attributes from the next page and runs no script.
        let html_class =
            Theme::renders_dark(cx, panel.is_some_and(|panel| panel.dark_mode)).then_some("dark");
        let favicon = panel
            .and_then(|panel| panel.brand.as_ref())
            .and_then(|brand| brand.favicon.clone());
        // Runtime navigation copies `lang` and `dir` with the class, so a page reads in its own
        // locale.
        let (_, locale) = crate::i18n::catalog_and_locale(cx);
        let (lang, dir) = (locale.id().clone(), locale.id().direction());
        Ok(view! {
            cx =>
            <!DOCTYPE html>
            <html lang=(lang) dir=(dir) class=(html_class)>
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
