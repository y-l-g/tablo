//! What one mounted panel serves from, and how a request finds its panel.
//!
//! [`RouterBuilderPanelExt::panel`](super::RouterBuilderPanelExt::panel) turns
//! each [`Panel`](super::Panel) into a [`PanelState`] and records it in the
//! router's [`Panels`]. The panel's layer puts its state on every request under
//! its prefix as [`CurrentPanel`], and every handler, the shell and the auth
//! gate read the panel through [`current`]: a router holds any number of
//! panels, and each request sees exactly one.

use std::{any::TypeId, collections::HashMap, sync::Arc};

use topcoat::context::{Cx, try_app_context, try_request_context};

use super::{
    relations::Child,
    shell::{Brand, ShellAssets},
};
use crate::{auth::Auth, navigation::NavigationItem, resource::Mounts, upload::InstalledUploader};

/// A sidebar entry and whether the request's user may open what it points at.
#[derive(Clone)]
pub(crate) struct NavEntry {
    pub(crate) item: NavigationItem,
    /// A resource's `ViewAny`, or a page's `Page::can_access`.
    pub(crate) visible: fn(&Cx) -> bool,
}

/// One mounted panel: its prefix, its shell, its auth, and the registries its
/// generic handlers dispatch through.
pub(crate) struct PanelState {
    /// The mount prefix, e.g. `/admin`: every URL the panel serves starts
    /// with it, and no other panel's prefix overlaps it.
    pub(crate) prefix: String,
    pub(crate) nav_items: Vec<NavEntry>,
    pub(crate) brand: Option<Brand>,
    pub(crate) dark_mode: bool,
    pub(crate) shell_assets: Option<ShellAssets>,
    /// Each registered resource's relation table, by resource type.
    pub(crate) children: HashMap<TypeId, Child>,
    /// The registered resources as the panel mounted them.
    pub(crate) mounts: Arc<Mounts>,
    /// Where the prefix redirects when the panel has no home page.
    pub(crate) root_redirect: Option<String>,
    pub(crate) auth: Auth,
    pub(crate) login_hint: Option<String>,
    pub(crate) uploads: Option<InstalledUploader>,
    /// The route patterns of the directories the panel serves.
    pub(crate) served_paths: Vec<String>,
    /// The URL each registered resource and page is served at, by type.
    pub(crate) urls: HashMap<TypeId, String>,
}

impl PanelState {
    /// Whether the panel requires a signed-in user.
    pub(crate) fn gates(&self) -> bool {
        !self.auth.is_disabled()
    }
}

/// Whether `path` is `prefix` or sits below it, segment by segment: `/admin`
/// serves `/admin`, `/admin/users` and `/admin?q=`, never `/administer`.
pub(crate) fn under_prefix(prefix: &str, path: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?']))
}

/// Every panel mounted on the router, in mount order: the app-context value
/// the auth gates read.
#[derive(Default)]
pub(crate) struct Panels(pub(crate) Vec<Arc<PanelState>>);

impl Panels {
    /// The panel mounted at exactly `prefix`.
    pub(crate) fn by_prefix(&self, prefix: &str) -> Option<&Arc<PanelState>> {
        self.0.iter().find(|panel| panel.prefix == prefix)
    }

    /// Whether every mounted panel requires a signed-in user.
    pub(crate) fn all_gate(&self) -> bool {
        self.0.iter().all(|panel| panel.gates())
    }

    /// Whether some mounted panel requires a signed-in user.
    pub(crate) fn any_gates(&self) -> bool {
        self.0.iter().any(|panel| panel.gates())
    }
}

/// The request's panel, as the request context carries it.
#[derive(Clone)]
pub(crate) struct CurrentPanel(pub(crate) Arc<PanelState>);

/// The request's panel: the one whose prefix the request is under, or the
/// panel whose session signed in a runtime request. A router with a single
/// panel answers it for any request.
pub(crate) fn current(cx: &Cx) -> Option<&Arc<PanelState>> {
    if let Some(CurrentPanel(panel)) = try_request_context::<CurrentPanel>(cx) {
        return Some(panel);
    }
    match try_app_context::<Panels>(cx).map(|panels| panels.0.as_slice()) {
        Some([only]) => Some(only),
        _ => None,
    }
}

/// The mounted panels, if the router mounts any.
pub(crate) fn panels(cx: &Cx) -> Option<&Panels> {
    try_app_context::<Panels>(cx)
}
