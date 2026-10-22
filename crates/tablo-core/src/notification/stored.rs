//! Stored notifications: the [`StoredNotification`] rows a panel user's bell lists, the
//! [`Notifications`] handle that sends them, and the [`Inbox`] every read and write of the bell
//! goes through.

use std::time::Duration;

use jiff::Timestamp;
use toasty::{Db, stmt::Expr};
use topcoat::context::{Cx, CxId, app_context, try_app_context};
use uuid::Uuid;

use super::{Notification, bus::NotificationBus};
use crate::panel::state::current;

/// Shipped record of a [`Notification`] kept for one panel user: what it says, whose it is, on
/// which panel and for which tenant, and whether they read it.
///
/// [`Notifications::send`] writes it and the bell of a panel with
/// [`Panel::notifications`](crate::Panel::notifications) lists it.
#[derive(Debug, Clone, toasty::Model)]
#[index(user_id, panel, created_at)]
pub struct StoredNotification {
    #[key]
    #[auto]
    pub id: Uuid,
    /// [`PanelUser::user_id`](crate::PanelUser::user_id) of the user it is for.
    pub user_id: String,
    /// The prefix of the panel whose bell lists it, e.g. `/admin`.
    pub panel: String,
    /// The tenant it belongs to: the bell lists it only while the user acts for that tenant.
    /// `None` lists it whichever tenant they act for.
    pub tenant: Option<Uuid>,
    /// What it says, as JSON, so a field a later release adds to [`Notification`] needs no
    /// migration.
    #[column(type = text)]
    pub notification: toasty::Json<Notification>,
    /// When the user marked it read; `None` while unread.
    pub read_at: Option<Timestamp>,
    pub created_at: Timestamp,
}

/// Sends [`StoredNotification`]s to one panel's users, and wakes their open bells. Cloning it is
/// cheap.
///
/// In a request, [`Notifications::of`] sends on the request's panel, for the request's tenant. A
/// job takes the handle from [`PanelHandle::notifications`](crate::PanelHandle::notifications).
///
/// ```rust,no_run
/// # use tablo_core::{Notification, Notifications, PanelUser, auth};
/// # use topcoat::context::Cx;
/// # async fn exported(cx: &Cx, owner: &dyn PanelUser) -> topcoat::Result<()> {
/// Notifications::of(cx)?
///     .send(
///         &owner.user_id(),
///         Notification::success("Your export is ready").action("Download", "/exports/42.csv"),
///     )
///     .await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct Notifications {
    db: Db,
    bus: NotificationBus,
    panel: String,
    tenant: Option<Uuid>,
    /// The request [`Self::of`] took it from; `None` for a job's handle.
    request: Option<CxId>,
}

impl Notifications {
    /// Sends on the request's panel, for the tenant the request acts for: the bell lists what it
    /// sends only while the recipient acts for the same tenant. [`tenant`](Self::tenant) changes
    /// it.
    ///
    /// # Errors
    ///
    /// A declaration error outside every panel, such as on a public page that did not
    /// [`enter`](crate::PanelHandle::enter) one.
    pub fn of(cx: &Cx) -> topcoat::Result<Self> {
        let Some(panel) = current(cx) else {
            return Err(crate::error::declaration(
                "Notifications::of needs a request under a panel".to_owned(),
            ));
        };
        // The pool itself, not `db(cx)`: a hook inside the write's transaction may take the
        // handle to `send_in` through it.
        let mut notifications = Self::new(
            app_context::<Db>(cx).clone(),
            bus(cx),
            panel.prefix.clone(),
            crate::tenant_id(cx),
        );
        notifications.request = Some(cx.id());
        Ok(notifications)
    }

    pub(crate) fn new(db: Db, bus: NotificationBus, panel: String, tenant: Option<Uuid>) -> Self {
        Self {
            db,
            bus,
            panel,
            tenant,
            request: None,
        }
    }

    /// Sends for `tenant` instead: the bell lists what this handle sends only while the
    /// recipient acts for it, or whichever tenant they act for with `None`.
    #[must_use]
    pub fn tenant(mut self, tenant: impl Into<Option<Uuid>>) -> Self {
        self.tenant = tenant.into();
        self
    }

    /// Stores `notification` for the panel user `user_id`, their
    /// [`PanelUser::user_id`](crate::PanelUser::user_id), unread, and wakes the bells they have
    /// open on the panel. It commits on its own connection: inside a write's transaction, such as
    /// in [`Resource::before_commit`](crate::Resource::before_commit), use [`Self::send_in`].
    ///
    /// # Errors
    ///
    /// An opaque infrastructure failure when the row cannot be written, e.g. the database does
    /// not register [`StoredNotification`]; the driver's text goes to the log.
    ///
    /// # Panics
    ///
    /// In a debug build, on a handle from [`Self::of`] while the request's panel write has its
    /// transaction open, as [`db`](crate::db::db) does.
    pub async fn send(
        &self,
        user_id: &str,
        notification: Notification,
    ) -> topcoat::Result<StoredNotification> {
        if let Some(request) = self.request {
            crate::db::assert_no_write_open(request, "Notifications::send");
        }
        let stored = self
            .store(&mut self.db.clone(), user_id, notification)
            .await?;
        self.bus.wake(&self.panel, user_id);
        Ok(stored)
    }

    /// Stores `notification` as [`Self::send`] does, through `ex`, so it exists only if the
    /// transaction `ex` runs commits. Within a panel write, such as in
    /// [`Resource::before_commit`](crate::Resource::before_commit) on a handle from
    /// [`Self::of`], the recipient's open bells wake once the write commits, and not at all if it
    /// rolls back; anywhere else they wake at once, and pick the row up after the commit when
    /// they next refresh.
    ///
    /// ```rust
    /// # use tablo_core::{Change, NoForm, Notification, Notifications, Resource};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, author: String }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// async fn before_commit(
    ///     cx: &Cx,
    ///     change: &Change<Post>,
    ///     ex: &mut dyn toasty::Executor,
    /// ) -> Result<()> {
    ///     if let Change::Update { after, .. } = change {
    ///         Notifications::of(cx)?
    ///             .send_in(
    ///                 &mut *ex,
    ///                 &after.author,
    ///                 Notification::info("Your post was edited"),
    ///             )
    ///             .await?;
    ///     }
    ///     Ok(())
    /// }
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// As [`Self::send`]. A failure inside a transaction leaves it to roll back.
    pub async fn send_in(
        &self,
        ex: &mut dyn toasty::Executor,
        user_id: &str,
        notification: Notification,
    ) -> topcoat::Result<StoredNotification> {
        let stored = self.store(ex, user_id, notification).await?;
        let (bus, panel, user_id) = (self.bus.clone(), self.panel.clone(), user_id.to_owned());
        let wake = move || bus.wake(&panel, &user_id);
        let unqueued = match self.request {
            Some(request) => crate::db::after_write_commits(request, wake),
            None => Err(wake),
        };
        if let Err(wake) = unqueued {
            wake();
        }
        Ok(stored)
    }

    async fn store(
        &self,
        ex: &mut dyn toasty::Executor,
        user_id: &str,
        notification: Notification,
    ) -> topcoat::Result<StoredNotification> {
        toasty::create!(StoredNotification {
            user_id: user_id.to_owned(),
            panel: self.panel.clone(),
            tenant: self.tenant,
            notification: toasty::Json(notification),
            read_at: None,
            created_at: Timestamp::now(),
        })
        .exec(ex)
        .await
        .map_err(failure)
    }

    /// Deletes the panel's stored notifications, read or not, sent more than `age` ago. Nothing
    /// prunes them otherwise; a job calls it on the schedule the app keeps them for.
    ///
    /// # Errors
    ///
    /// An opaque infrastructure failure when the rows cannot be deleted.
    pub async fn prune(&self, age: Duration) -> topcoat::Result<()> {
        let cutoff = Timestamp::now()
            .checked_sub(jiff::SignedDuration::try_from(age).map_err(topcoat::Error::from)?)
            .map_err(topcoat::Error::from)?;
        let fields = StoredNotification::fields();
        StoredNotification::filter(
            fields
                .panel()
                .eq(self.panel.clone())
                .and(fields.created_at().lt(cutoff)),
        )
        .delete()
        .exec(&mut self.db.clone())
        .await
        .map_err(failure)
    }
}

/// A failed read or write of stored notifications: the driver's text goes to the log, never to
/// the page.
fn failure(error: toasty::Error) -> topcoat::Error {
    crate::error::driver_failure(error, "Notifications are unavailable right now.")
}

/// The router's bus, or one no bell listens on outside a router.
pub(crate) fn bus(cx: &Cx) -> NotificationBus {
    try_app_context::<NotificationBus>(cx)
        .cloned()
        .unwrap_or_else(NotificationBus::new)
}

/// The stored notifications the signed-in user's bell answers for: theirs, on the request's
/// panel, for the tenant the request acts for or for none. Every read and write the bell makes
/// filters through it, never through an id alone.
#[derive(Clone)]
pub(crate) struct Inbox {
    pub(crate) panel: String,
    pub(crate) user_id: String,
    tenant: Option<Uuid>,
}

impl Inbox {
    /// The signed-in user's inbox; `None` without one.
    pub(crate) fn of(cx: &Cx) -> Option<Self> {
        let signed = crate::auth::signed(cx)?;
        Some(Self {
            panel: signed.panel.prefix.clone(),
            user_id: signed.user.user_id(),
            tenant: crate::tenant_id(cx),
        })
    }

    /// The rows this inbox holds.
    fn holds(&self) -> Expr<bool> {
        let fields = StoredNotification::fields();
        let tenant = match self.tenant {
            Some(tenant) => fields.tenant().is_none().or(fields.tenant().eq(tenant)),
            None => fields.tenant().is_none(),
        };
        fields
            .user_id()
            .eq(self.user_id.clone())
            .and(fields.panel().eq(self.panel.clone()))
            .and(tenant)
    }

    /// The row `id` names, when this inbox holds it.
    fn holds_id(&self, id: Uuid) -> Expr<bool> {
        StoredNotification::fields().id().eq(id).and(self.holds())
    }

    /// Up to `limit` rows, newest first.
    pub(crate) async fn latest(
        &self,
        db: &Db,
        limit: usize,
    ) -> topcoat::Result<Vec<StoredNotification>> {
        StoredNotification::filter(self.holds())
            .order_by(StoredNotification::fields().created_at().desc())
            .limit(limit)
            .exec(&mut db.clone())
            .await
            .map_err(failure)
    }

    /// How many rows are unread.
    pub(crate) async fn unread(&self, db: &Db) -> topcoat::Result<u64> {
        StoredNotification::filter(
            self.holds()
                .and(StoredNotification::fields().read_at().is_none()),
        )
        .count()
        .exec(&mut db.clone())
        .await
        .map_err(failure)
    }

    /// Marks the row `id` names read, or unread with `read` false.
    pub(crate) async fn mark(&self, db: &Db, id: Uuid, read: bool) -> topcoat::Result<()> {
        StoredNotification::filter(self.holds_id(id))
            .update()
            .read_at(read.then(Timestamp::now))
            .exec(&mut db.clone())
            .await
            .map_err(failure)
    }

    /// Marks every unread row read.
    pub(crate) async fn mark_all_read(&self, db: &Db) -> topcoat::Result<()> {
        StoredNotification::filter(
            self.holds()
                .and(StoredNotification::fields().read_at().is_none()),
        )
        .update()
        .read_at(Some(Timestamp::now()))
        .exec(&mut db.clone())
        .await
        .map_err(failure)
    }

    /// Deletes the row `id` names.
    pub(crate) async fn remove(&self, db: &Db, id: Uuid) -> topcoat::Result<()> {
        StoredNotification::filter(self.holds_id(id))
            .delete()
            .exec(&mut db.clone())
            .await
            .map_err(failure)
    }

    /// Deletes every row.
    pub(crate) async fn clear(&self, db: &Db) -> topcoat::Result<()> {
        StoredNotification::filter(self.holds())
            .delete()
            .exec(&mut db.clone())
            .await
            .map_err(failure)
    }
}
