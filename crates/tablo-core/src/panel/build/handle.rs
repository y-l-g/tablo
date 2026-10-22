//! [`PanelHandle`]: a mounted panel kept for a job, a test or a public page.

use std::sync::Arc;

use toasty::Db;
use topcoat::{
    Result,
    context::Cx,
    view::{BoxView, View, ViewExt},
};

use super::validation_cx;
use crate::{
    bus::NotificationBus,
    notification::Notifications,
    panel::{
        Panel,
        state::{CurrentPanel, PanelState, current},
    },
    resource::Mounts,
};

/// A panel's mounted resources and its database, which a background job or a test keeps to build
/// a context per run without declaring the panel again. A router's mounted panel gives it through
/// [`panel_handle`](super::RouterBuilderPanelExt::panel_handle), a panel no router mounts through
/// [`Panel::handle`]. Cloning it is cheap.
///
/// ```rust,no_run
/// # use tablo_core::{NoForm, Panel, Resource, RouterBuilderPanelExt, Tenant, scoped_query};
/// # use topcoat::router::{Router, RouterBuilderDiscoverExt};
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
/// # struct PostResource;
/// # impl Resource for PostResource { type Model = Post; type Form = NoForm<Post>; }
/// # async fn run(db: toasty::Db, tenant: uuid::Uuid) -> topcoat::Result<()> {
/// let builder = Router::builder()
///     .discover()
///     .app_context(db)
///     .panel(Panel::new("admin").resource::<PostResource>())?;
/// let admin = builder.panel_handle("admin").expect("mounted above");
/// let router = builder.build();
///
/// // In the job, once per run:
/// let cx = admin.context().with(Tenant(tenant));
/// let posts = scoped_query::<PostResource>(&cx)?;
/// # let _ = (router, posts);
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct PanelHandle {
    pub(super) db: Db,
    pub(super) mounts: Arc<Mounts>,
    /// The mounted panel, for a handle a router gives; `None` for [`Panel::handle`]'s.
    pub(super) panel: Option<Arc<PanelState>>,
    pub(super) prefix: String,
    /// The router's bus, or one no bell listens on for [`Panel::handle`]'s.
    pub(super) bus: NotificationBus,
}

impl PanelHandle {
    /// A context outside any request in which the panel's resources answer as the panel mounted
    /// them: what a job or a test passes to [`scoped_query`](crate::scoped_query),
    /// [`can`](crate::can), [`write_create`](crate::write_create) and the other entry points that
    /// answer from a mounted def. It holds the database and the resources, and no request,
    /// session or tenant. Add a tenant with `cx.with(Tenant(id))`. It carries no signed-in user,
    /// so a policy that reads one answers as for a signed-out request.
    ///
    /// The context is one unit of work, as a request is: loads it memoizes stay cached for its
    /// lifetime, so a job builds one per run. A context built with no panel at all knows no
    /// resource: every entry point that reads a mounted def answers as for a resource the panel
    /// does not mount.
    pub fn context(&self) -> Cx {
        validation_cx(&self.db, &self.mounts)
    }

    /// `cx` answering as this panel, for a request outside every panel's prefix, such as a
    /// public page: [`scoped_query`](crate::scoped_query), [`can`](crate::can) and the other
    /// entry points that answer from a mounted def answer as this panel mounted its resources,
    /// and, for a handle a router gives, the [`url`](crate::url) helpers answer with its URLs.
    /// A request outside every prefix answers as no panel until it enters one, however many
    /// panels the router mounts. It runs no auth gate and signs no one in, so a public page's
    /// context knows no user. Enter only outside every prefix: under a panel's prefix, a handle
    /// [`Panel::handle`] gives leaves the request answering as its own panel.
    ///
    /// ```rust,no_run
    /// # use tablo_core::{NoForm, PanelHandle, Resource, scoped_query};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// # struct PostResource;
    /// # impl Resource for PostResource { type Model = Post; type Form = NoForm<Post>; }
    /// async fn public_posts(cx: &Cx, admin: &PanelHandle) -> Result<Vec<Post>> {
    ///     let cx = admin.enter(cx);
    ///     Ok(scoped_query::<PostResource>(&cx)?
    ///         .exec(&mut tablo_core::db::db(&cx))
    ///         .await?)
    /// }
    /// ```
    pub fn enter(&self, cx: &Cx) -> Cx {
        match &self.panel {
            Some(panel) => cx.with(CurrentPanel(Arc::clone(panel))),
            None => cx.with(EnteredPanel(Arc::clone(&self.mounts))),
        }
    }

    /// The panel's prefix, e.g. `/admin`, for a public page's link to it; `None` for
    /// [`Panel::handle`]'s, which no router mounts.
    pub fn url(&self) -> Option<String> {
        self.panel.as_ref().map(|panel| panel.realm.prefix.clone())
    }

    /// Sends stored notifications to this panel's users from outside a request, such as from a
    /// job, for no tenant until [`Notifications::tenant`] names one. The bells they have open
    /// update when the handle is a router's.
    ///
    /// ```rust,no_run
    /// # use tablo_core::{Notification, PanelHandle};
    /// # async fn nightly(admin: &PanelHandle, owner_id: &str, tenant: uuid::Uuid) -> topcoat::Result<()> {
    /// admin
    ///     .notifications()
    ///     .tenant(tenant)
    ///     .send(owner_id, Notification::info("The nightly import finished"))
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn notifications(&self) -> Notifications {
        Notifications::new(self.db.clone(), self.bus.clone(), self.prefix.clone(), None)
    }

    /// Renders a complete HTML document around a page outside every prefix with this panel's
    /// shell settings: its stylesheet, font, scripts, favicon and dark-mode default. A handle
    /// [`Panel::handle`] gives renders the document without them.
    ///
    /// ```rust,no_run
    /// # use tablo_core::PanelHandle;
    /// # use topcoat::{Result, context::{Cx, app_context}, router::{Slot, layout}, view::{View, view}};
    /// /// The handle `panel_handle("admin")` returned, kept in the app context.
    /// struct Admin(PanelHandle);
    ///
    /// #[layout("/blog")]
    /// async fn blog_layout(cx: &Cx, slot: Slot<'_>) -> Result<impl View> {
    ///     let Admin(admin) = app_context::<Admin>(cx);
    ///     admin.document(cx, "Blog", view! { <main>(slot)</main> }).await
    /// }
    /// ```
    pub async fn document<'a>(
        &'a self,
        cx: &'a Cx,
        title: impl Into<String>,
        body: impl View + 'a,
    ) -> Result<BoxView<'a>> {
        Panel::render_document(cx, self.panel.as_deref(), title.into(), body.boxed()).await
    }
}

/// The resources [`PanelHandle::enter`] put in scope for a request outside every panel.
pub(super) struct EnteredPanel(Arc<Mounts>);

/// The resources the request answers from: its own panel's, or the ones
/// [`PanelHandle::enter`] put in scope.
pub(super) fn request_mounts(cx: &Cx) -> Option<&Mounts> {
    match current(cx) {
        Some(panel) => Some(&*panel.mounts),
        None => topcoat::context::try_request_context::<EnteredPanel>(cx)
            .map(|EnteredPanel(mounts)| &**mounts),
    }
}
