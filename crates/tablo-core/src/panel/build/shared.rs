//! What every panel on a router shares, installed with the first one.

use topcoat::{
    cookie::CookieLayer,
    router::RouterBuilder,
    runtime::{PrefetchMode, RuntimeLayer},
};

use super::request_mounts;
use crate::{
    auth::{AuthHost, PagePanelStamp, RuntimeGate},
    bus::NotificationBus,
    panel::{host::PanelHost, state::Panels},
    resource::MountScope,
    settle::AfterResponse,
    tenancy::TenantSource,
};

/// Installs what every panel on a router shares.
pub(super) fn install_shared(mut builder: RouterBuilder) -> RouterBuilder {
    builder = builder
        .layer(CookieLayer::new())
        .layer(RuntimeGate::new())
        .with(Panels::default())
        .with(AuthHost(&PanelHost))
        .with(MountScope(request_mounts))
        .with(TenantSource(crate::auth::session_tenant))
        .with(crate::values::ZoneSource(crate::auth::session_time_zone))
        .with(crate::schema::FileHref(crate::panel::url::file))
        .with(crate::page::PageBody(crate::panel::pages::declared_body))
        .with(crate::i18n::CatalogSource(crate::auth::session_catalog))
        .with(NotificationBus::new())
        .with(AfterResponse::default());
    // The runtime layer has no path, so a page re-run reaches the panel's layers already rewritten
    // to a `GET`. One the app installs before the panels sits inside this one and passes on what
    // this one already rewrote.
    builder = builder.layer(RuntimeLayer::new());
    if builder.try_context::<PrefetchMode>().is_none() {
        builder = builder.with(PrefetchMode::Never);
    }
    // The last pathless layer runs outermost, so this one sees the socket's handshake unless the
    // app installs the runtime layer after the panels. `RuntimeGate` refuses those sockets' runs.
    builder.layer(PagePanelStamp)
}
