//! The panel's home page, served at the panel prefix by `Panel::home`.

use tablo_core::Page;
use topcoat::{
    Result,
    context::Cx,
    view::{View, view},
};

/// The page `/admin` serves.
pub struct Dashboard;

impl Page for Dashboard {
    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! {
            cx =>
            tablo_ui::page(
                tablo_ui::page_header(
                    tablo_ui::page_title("Dashboard")
                    tablo_ui::page_description(
                        "The blog's admin: its users, authors, posts, comments and media."
                    )
                )
            )
        })
    }
}
