//! The panel as auth's [`Host`]: auth reads the request's realm and renders its pages through it,
//! and names no panel type.

use std::{collections::HashSet, sync::Arc};

use topcoat::{context::Cx, view::BoxView};

use super::{
    Panel,
    forms::{FormChrome, render_form_page},
    gate::panel_prefix,
    state::{CurrentPanel, current, panels},
};
use crate::{
    BoxFuture,
    auth::{Host, Realm, SettingsPage},
};

/// The [`Host`] every router that mounts a panel installs.
pub(crate) struct PanelHost;

impl Host for PanelHost {
    fn realm<'a>(&self, cx: &'a Cx) -> Option<&'a Arc<Realm>> {
        current(cx).map(|panel| &panel.realm)
    }

    fn realms(&self, cx: &Cx) -> Vec<Arc<Realm>> {
        panels(cx).map_or_else(Vec::new, |panels| {
            panels
                .0
                .iter()
                .map(|panel| Arc::clone(&panel.realm))
                .collect()
        })
    }

    fn declaring(&self, cx: &Cx, path: &str) -> Vec<Arc<Realm>> {
        panels(cx).map_or_else(Vec::new, |panels| {
            panels
                .declaring(path)
                .map(|panel| Arc::clone(&panel.realm))
                .collect()
        })
    }

    fn enter(&self, cx: &Cx, realm: &Arc<Realm>) -> Option<Cx> {
        let panel = panels(cx)?
            .0
            .iter()
            .find(|panel| Arc::ptr_eq(&panel.realm, realm))?;
        Some(cx.with(CurrentPanel(Arc::clone(panel))))
    }

    fn answer(&self, cx: &Cx, error: topcoat::Error) -> topcoat::Error {
        super::error_page::answer(cx, error)
    }

    fn brand_name(&self, cx: &Cx) -> String {
        Panel::brand_name(cx)
    }

    fn page_brand<'a>(&self, cx: &'a Cx) -> BoxView<'a> {
        Panel::render_page_brand(cx)
    }

    fn document<'a>(
        &self,
        cx: &'a Cx,
        title: String,
        body: BoxView<'a>,
    ) -> BoxFuture<'a, topcoat::Result<BoxView<'a>>> {
        Box::pin(Panel::document(cx, title, body))
    }

    fn settings_page<'a: 'f, 'f>(
        &self,
        cx: &'a Cx,
        page: SettingsPage<'f>,
    ) -> BoxFuture<'f, topcoat::Result<BoxView<'a>>> {
        let SettingsPage {
            title,
            schema,
            hidden,
            values,
            errors,
        } = page;
        let mut chrome = FormChrome::settings(title, panel_prefix(cx));
        if let Some((name, value)) = hidden {
            chrome = chrome.with_hidden(name, value);
        }
        Box::pin(async move {
            render_form_page(cx, schema, chrome, values, errors, &HashSet::new()).await
        })
    }
}
