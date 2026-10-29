//! The toast stack every panel page renders, with or without the shell.

use topcoat::{
    Result,
    context::Cx,
    view::{BoxView, ViewExt, view},
};

use crate::{
    notification::{LiveToast, live_toast, live_toaster, take_notification},
    panel::Panel,
};

impl Panel {
    /// The toast stack, a polite live region fixed bottom-right: the flash cookie's toast beside
    /// `live_toaster`, the page-owned in-place transport. It hoists, so it renders in a page body.
    pub(super) async fn toaster(cx: &Cx) -> Result<BoxView<'_>> {
        let notification_view: BoxView<'_> = match take_notification(cx) {
            Some(notification) => {
                crate::notification::render_notification(cx, notification, Default::default())
                    .await?
            }
            // No placeholder: the toaster's `<ol>` permits only `li`/`script`/`template` children.
            None => ().boxed(),
        };
        // The page owns the live-toast signals; the same helper resolves the same handles here.
        let LiveToast {
            status: toast_status,
            title: toast_title,
            description: toast_description,
            serial: toast_serial,
        } = live_toast(cx);
        Ok(view! {
            cx =>
            tablo_ui::toaster(
                label: crate::t!(cx, "tablo-notifications"),
                (notification_view)
                live_toaster(
                    status: $(toast_status),
                    title: $(toast_title),
                    description: $(toast_description),
                    serial: $(toast_serial)
                )
            )
        }
        .boxed())
    }
}
