//! [`Panel::environment`]: the deployment a panel runs in, named on every page so nobody mistakes
//! staging for production.

use topcoat::{
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use super::super::Panel;
use crate::{Color, Text};

/// The deployment a panel runs in, such as staging: every page of the panel, its sign-in pages
/// included, carries a stripe and a badge in its color, and its browser tab title starts with its
/// name.
///
/// ```rust
/// # use tablo_core::{Color, Environment, Panel};
/// let panel = Panel::new("admin");
/// let panel = match std::env::var("APP_ENV").as_deref() {
///     Ok("production") => panel,
///     Ok(name) => panel.environment(Environment::new(name.to_owned()).color(Color::Info)),
///     Err(_) => panel.environment(Environment::new("Local")),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct Environment {
    name: Text,
    color: Color,
}

impl Environment {
    /// An environment named `name`, in [`Color::Warning`].
    pub fn new(name: impl Into<Text>) -> Self {
        Self {
            name: name.into(),
            color: Color::Warning,
        }
    }

    /// Paint the stripe and the badge in `color`.
    #[must_use]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// The name, in the request's locale.
    pub(crate) fn name(&self, cx: &Cx) -> String {
        self.name.localize(cx)
    }

    /// The stripe along the top of the page and the badge at its bottom: fixed, so neither moves
    /// the page's layout, and the badge lets clicks through to what it covers.
    pub(crate) fn ribbon<'a>(&self, cx: &'a Cx) -> BoxView<'a> {
        let name = self.name(cx);
        let stripe = format!(
            "pointer-events-none fixed inset-x-0 top-0 z-50 h-0.5 {}",
            self.color.fill_class()
        );
        let badge = format!(
            "pointer-events-none fixed bottom-3 left-1/2 z-50 -translate-x-1/2 rounded-full px-3 \
             py-0.5 text-xs font-medium text-white shadow-sm {}",
            self.color.fill_class()
        );
        view! {
            <div class=(stripe) aria-hidden="true"></div>
            <div class=(badge) role="note" data-environment=(name.clone())>(name)</div>
        }
        .boxed()
    }
}

impl Panel {
    /// Names the deployment the panel runs in on every page: a stripe and a badge, and the name
    /// at the start of the browser tab title. Production usually names none.
    #[must_use]
    pub fn environment(mut self, environment: Environment) -> Self {
        self.environment = Some(environment);
        self
    }
}
