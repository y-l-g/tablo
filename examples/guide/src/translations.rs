//! The Translations chapter's snippets.

use tablo::{Notification, Panel, prelude::*};
use topcoat::{
    context::Cx,
    l10n::{Locale, locale},
};

use crate::models::Post;

pub fn panel() -> Panel {
    // ANCHOR: locales
    Panel::new("admin")
        .locales([locale!("fr"), locale!("en")])
        .translations(locale!("fr"), include_str!("../locales/fr.ftl"))
        .translations(locale!("en"), include_str!("../locales/en.ftl"))
    // ANCHOR_END: locales
}

pub fn title() -> TextColumn<Post, String> {
    // ANCHOR: label
    TextColumn::new(lens!(Post.title)).label(text!("post-headline"))
    // ANCHOR_END: label
}

pub fn published(cx: &Cx, count: u64) -> Notification {
    // ANCHOR: t
    Notification::success(t!(cx, "posts-published", count = count))
    // ANCHOR_END: t
}

// ANCHOR: user-locale
pub struct Editor {
    pub id: String,
    pub locale: Option<String>,
}

impl tablo::PanelUser for Editor {
    fn user_id(&self) -> String {
        self.id.clone()
    }

    fn display_name(&self) -> &str {
        &self.id
    }

    fn locale(&self) -> Option<Locale> {
        self.locale.as_deref()?.parse().ok()
    }
}
// ANCHOR_END: user-locale
