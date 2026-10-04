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
    search::{RelationSearchFn, SearchFn},
    shell::{Brand, ShellAssets},
};
use crate::{auth::Auth, resource::NavigationItem, upload::InstalledUploader};

/// One mounted panel: its prefix, its shell, its auth, and the registries its
/// generic handlers dispatch through.
pub(crate) struct PanelState {
    /// The mount prefix, e.g. `/admin`: every URL the panel serves starts
    /// with it, and no other panel's prefix overlaps it.
    pub(crate) prefix: String,
    pub(crate) nav_items: Vec<NavigationItem>,
    pub(crate) brand: Option<Brand>,
    pub(crate) dark_mode: bool,
    pub(crate) shell_assets: Option<ShellAssets>,
    /// Live-search loaders by list path.
    pub(crate) search: HashMap<String, SearchFn>,
    /// Relation live-search loaders by (parent slug, child slug).
    pub(crate) relations: HashMap<(String, String), RelationSearchFn>,
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

    /// Whether `path` is the prefix itself or a path below it.
    pub(crate) fn serves(&self, path: &str) -> bool {
        under_prefix(&self.prefix, path)
    }
}

/// Whether `path` is `prefix` or sits below it, segment by segment: `/admin`
/// serves `/admin`, `/admin/users` and `/admin?q=`, never `/administer`.
pub(crate) fn under_prefix(prefix: &str, path: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?']))
}

/// Every panel mounted on the router, in mount order: the app-context value
/// the auth gates and the shard dispatch read.
#[derive(Default)]
pub(crate) struct Panels(pub(crate) Vec<Arc<PanelState>>);

impl Panels {
    /// The panel mounted at exactly `prefix`.
    pub(crate) fn by_prefix(&self, prefix: &str) -> Option<&Arc<PanelState>> {
        self.0.iter().find(|panel| panel.prefix == prefix)
    }

    /// The panel serving `path`. Prefixes never overlap, so at most one does.
    pub(crate) fn by_path(&self, path: &str) -> Option<&Arc<PanelState>> {
        self.0.iter().find(|panel| panel.serves(path))
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

/// The request's panel: the one whose prefix the request is under, the one a
/// live table's shard re-renders for, or the panel whose session signed in a
/// runtime request. A router with a single panel answers it for any request.
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
