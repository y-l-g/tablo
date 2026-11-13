//! What one mounted panel serves from, and how a request finds its panel.
//!
//! [`RouterBuilderPanelExt::panel`](super::RouterBuilderPanelExt::panel) turns
//! each [`Panel`](super::Panel) into a [`PanelState`] and records it in the
//! router's [`Panels`]. The auth gate [`enter`]s the panel on every request
//! under its prefix, and every handler and the shell read the panel through
//! [`current`]: a router holds any number of panels, and each request sees
//! exactly one. Auth reads the same panel's [`Realm`], which `enter` installs
//! beside it.

use std::{any::TypeId, collections::HashMap, sync::Arc};

use topcoat::context::{Cx, try_context};

use super::{
    relations::Child,
    shell::{Brand, ShellAssets},
};
use crate::{
    auth::{CurrentRealm, Realm},
    navigation::NavigationItem,
    page::MountedPage,
    resource::Mounts,
    upload::InstalledUploader,
};

/// A sidebar entry and whether the request's user may open what it points at.
#[derive(Clone)]
pub(crate) struct NavEntry {
    pub(crate) item: NavigationItem,
    /// A resource's `ViewAny`, or a page's `PageDef::access`.
    pub(crate) visible: Arc<dyn Fn(&Cx) -> bool + Send + Sync>,
    /// A resource's [`navigation_badge`](crate::ResourceDef::navigation_badge), counted per
    /// request.
    pub(crate) badge: Option<BadgeFn>,
    /// The area whose sidebar lists the entry; `None` lists it in the panel's.
    pub(crate) area: Option<AreaSlot>,
}

/// Where in an [`Area`](crate::Area) a sidebar entry sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AreaSlot {
    /// The area's index in [`PanelState::areas`].
    pub(crate) area: usize,
    /// Whether the entry sits in the area's tenant group, headed by the tenant's name.
    pub(crate) tenant: bool,
}

/// One mounted [`Area`](crate::Area): where it is served, and the first step of its trail.
#[derive(Debug, Clone)]
pub(crate) struct AreaNav {
    /// `{prefix}/{slug}`, which every page of the area is under.
    pub(crate) url: String,
    pub(crate) label: crate::Text,
}

/// What the trail and the sidebar read of a resource's pages, for every resource the panel mounts:
/// one the sidebar lists names its own entry, and one it does not is placed under the resource
/// whose relation lists its records.
#[derive(Debug, Clone)]
pub(crate) struct ResourceTrail {
    pub(crate) resource: TypeId,
    /// The list's path.
    pub(crate) list: String,
    /// The resource's plural label, which names the list of a resource the sidebar does not list.
    pub(crate) label: crate::Text,
    /// Whether the sidebar lists the resource.
    pub(crate) listed: bool,
    /// Whether the resource has a detail page, which the trail links a record's title to.
    pub(crate) viewed: bool,
    /// Whether the resource has an edit page, which the trail links a record's title to when it
    /// has no detail page.
    pub(crate) edited: bool,
    /// The labels of the record pages and relation tabs under a record, by slug.
    pub(crate) pages: Vec<(String, crate::Text)>,
    /// The resources the resource's relations list, with the column each child row holds the
    /// owner's key in and the slug of the relation's tab.
    pub(crate) owns: Vec<Owned>,
    /// Loads a record's title and form values, `None` when the request may not view it.
    pub(crate) record: RecordFn,
}

/// One relation of a [`ResourceTrail`]'s resource, as the trail of its child reads it.
#[derive(Debug, Clone)]
pub(crate) struct Owned {
    pub(crate) child: TypeId,
    /// The child's form key holding the owner's key; `None` for a many-to-many relation.
    pub(crate) foreign_key: Option<String>,
    /// The relation's tab under an owner record, by the child's slug.
    pub(crate) tab: Option<String>,
}

/// What the trail reads of one record.
pub(crate) struct RecordFacts {
    pub(crate) title: String,
    /// The record as its form spells it, where a child reads its owner's key.
    pub(crate) values: crate::form::FormValues,
}

/// Loads the record a key names, scoped and policy-checked as its detail page loads it.
pub(crate) type RecordFn = for<'a> fn(&'a Cx, &'a str) -> crate::BoxFuture<'a, Option<RecordFacts>>;

/// A resource the panel's global search searches, its type erased.
#[derive(Clone)]
pub(crate) struct Searchable {
    /// The resource's plural label, which heads its results.
    pub(crate) label: crate::Text,
    /// The resource's list URL, which places its results in the sidebar's order.
    pub(crate) list: String,
    /// Whether the request may list the resource.
    pub(crate) visible: fn(&Cx) -> bool,
    pub(crate) search: SearchFn,
}

/// Up to the given number of a resource's records matching every word, as results.
pub(crate) type SearchFn =
    for<'a> fn(
        &'a Cx,
        &'a [String],
        usize,
    ) -> crate::BoxFuture<'a, topcoat::Result<Vec<crate::resource::SearchHit>>>;

/// The badge text a sidebar entry counts for a request; `None` shows no badge.
pub(crate) type BadgeFn =
    Arc<dyn for<'a> Fn(&'a Cx) -> crate::BoxFuture<'a, Option<String>> + Send + Sync>;

/// One mounted panel: its realm, its shell, and the registries its generic handlers dispatch
/// through.
pub(crate) struct PanelState {
    /// What auth serves of the panel, its prefix among it: every URL the panel serves starts with
    /// the prefix, and no other panel's prefix overlaps it.
    pub(crate) realm: Arc<Realm>,
    pub(crate) nav_items: Vec<NavEntry>,
    /// Every mounted resource's trail, in registration order.
    pub(crate) trails: Vec<ResourceTrail>,
    /// The panel's areas, in registration order; [`AreaSlot::area`] indexes it.
    pub(crate) areas: Vec<AreaNav>,
    pub(crate) brand: Option<Brand>,
    pub(crate) dark_mode: bool,
    /// The deployment the panel runs in, which every page names.
    pub(crate) environment: Option<super::Environment>,
    pub(crate) shell_assets: Option<ShellAssets>,
    /// Each registered resource's relation table, by resource type.
    pub(crate) children: HashMap<TypeId, Child>,
    /// The registered resources as the panel mounted them.
    pub(crate) mounts: Arc<Mounts>,
    /// Where the prefix redirects when the panel has no home page.
    pub(crate) root_redirect: Option<String>,
    /// The app's entries in the user menu.
    pub(crate) user_menu: Vec<NavigationItem>,
    /// The app's banners, in the order the panel added them.
    pub(crate) banners: Vec<super::banner::BannerFn>,
    pub(crate) uploads: Option<InstalledUploader>,
    /// The route patterns of the directories the panel serves.
    pub(crate) served_paths: Vec<String>,
    /// The URL prefixes, such as `/uploads/`, of the directories served at signed URLs only.
    pub(crate) signed_roots: Vec<String>,
    /// The URL each registered resource and page is served at, by type.
    pub(crate) urls: HashMap<TypeId, String>,
    /// The registered pages as the panel mounted them, by type.
    pub(crate) mounted_pages: HashMap<TypeId, Arc<MountedPage>>,
    /// The paths of the runtime endpoints the panel declared with
    /// [`Panel::endpoint`](super::Panel::endpoint).
    pub(crate) endpoints: Vec<String>,
    /// The resources the global search searches, in the sidebar's order; empty when the panel
    /// shows no global search.
    pub(crate) searches: Vec<Searchable>,
    pub(crate) limits: crate::Limits,
    /// The hooks every write of the panel runs before it commits, in registration order.
    pub(crate) commit_hooks: Vec<super::commit_hook::CommitHookFn>,
    /// What the panel loads for each request it admits, in registration order.
    pub(crate) loads: Vec<super::load::LoadFn>,
}

/// Whether `path` is `prefix` or sits below it, segment by segment: `/admin`
/// serves `/admin`, `/admin/users` and `/admin?q=`, never `/administer`.
pub(crate) fn under_prefix(prefix: &str, path: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['/', '?']))
}

impl PanelState {
    /// The index of the area `path` is under; `None` when it is under none.
    pub(crate) fn area_at(&self, path: &str) -> Option<usize> {
        self.areas
            .iter()
            .position(|area| under_prefix(&area.url, path))
    }
}

/// Every panel mounted on the router, in mount order: the router context value the panel's
/// [`Host`](crate::auth::Host) answers from.
#[derive(Default, Clone)]
pub(crate) struct Panels(pub(crate) Vec<Arc<PanelState>>);

impl Panels {
    /// The panels that declared the runtime endpoint at `path`, in mount order. A trailing slash
    /// does not count: under [`TrailingSlash::Serve`](topcoat::router::TrailingSlash::Serve) the
    /// endpoint answers both forms of its path.
    pub(crate) fn declaring<'a>(
        &'a self,
        path: &'a str,
    ) -> impl Iterator<Item = &'a Arc<PanelState>> {
        let bare = |path: &'a str| path.strip_suffix('/').unwrap_or(path);
        self.0.iter().filter(move |panel| {
            panel
                .endpoints
                .iter()
                .any(|endpoint| bare(endpoint) == bare(path))
        })
    }
}

/// The request's panel, as the request's context carries it.
#[derive(Clone)]
pub(crate) struct CurrentPanel(pub(crate) Arc<PanelState>);

/// The request's panel: the one whose prefix the request is under, or, for a
/// runtime request, the panel that declared its endpoint or whose session sent it. A request
/// outside every panel has none, however many panels the router mounts.
pub(crate) fn current(cx: &Cx) -> Option<&Arc<PanelState>> {
    try_context::<CurrentPanel>(cx).map(|CurrentPanel(panel)| panel)
}

/// `cx` as a request to `panel`: its state, and its realm for auth.
pub(crate) fn enter(cx: &Cx, panel: &Arc<PanelState>) -> Cx {
    let cx = crate::i18n::entered(cx, &panel.realm.catalog);
    cx.with_many((
        CurrentPanel(Arc::clone(panel)),
        CurrentRealm(Arc::clone(&panel.realm)),
        panel.limits,
        super::list::ShownList::default(),
    ))
}

/// The uploader the request's panel was mounted with, if it has one.
pub(crate) fn uploader(cx: &Cx) -> Option<&InstalledUploader> {
    current(cx)?.uploads.as_ref()
}

/// The mounted panels, if the router mounts any.
pub(crate) fn panels(cx: &Cx) -> Option<&Panels> {
    try_context::<Panels>(cx)
}
