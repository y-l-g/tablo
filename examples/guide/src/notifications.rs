//! The Notifications chapter's snippets.

use std::time::Duration;

use tablo::{Notification, Notifications, Panel, PanelHandle};
use topcoat::{Result, context::Cx};

pub fn panel() -> Panel {
    // ANCHOR: enable
    Panel::new("admin").notifications()
    // ANCHOR_END: enable
}

pub async fn export_finished(cx: &Cx, owner_id: &str, export: u64) -> Result<()> {
    // ANCHOR: send
    Notifications::of(cx)?
        .send(
            owner_id,
            Notification::success("Your export is ready")
                .description("It holds every order of the quarter.")
                .action("Download", format!("/exports/{export}.csv")),
        )
        .await?;
    // ANCHOR_END: send
    Ok(())
}

pub async fn nightly_import(admin: &PanelHandle, owner_id: &str, tenant: uuid::Uuid) -> Result<()> {
    // ANCHOR: job
    admin
        .notifications()
        .tenant(tenant)
        .send(owner_id, Notification::info("The nightly import finished"))
        .await?;
    // ANCHOR_END: job
    Ok(())
}

pub async fn prune(admin: &PanelHandle) -> Result<()> {
    // ANCHOR: prune
    admin
        .notifications()
        .prune(Duration::from_hours(24 * 90))
        .await?;
    // ANCHOR_END: prune
    Ok(())
}
