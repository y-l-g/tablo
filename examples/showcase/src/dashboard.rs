//! The panel's home page, served at the panel prefix by `Panel::home`.

use tablo_core::{NavigationItem, Page, Resource, can_list, db::db, scoped_query};
use topcoat::{
    Result,
    context::Cx,
    icon::{IconData, icon},
    view::{BoxView, View, ViewExt, view},
};

use crate::app::{AuthorResource, CommentResource, PostResource, UserResource};

/// The page the panel serves at its prefix.
pub struct Dashboard;

/// One resource's tile: its label, its list, and how many records the
/// caller can see there.
struct Stat {
    label: String,
    url: String,
    icon: IconData,
    /// `None` when the count could not be read: the tile shows a dash rather
    /// than a wrong zero.
    count: Option<u64>,
}

/// `R`'s tile, or `None` when the caller may not open `R`'s list, so the
/// dashboard links to no list that would answer 403, or when the panel does
/// not register `R`.
async fn stat<R: Resource>(cx: &Cx, glyph: IconData) -> Option<Stat> {
    if !can_list::<R>(cx) {
        return None;
    }
    let url = tablo_core::url::resource::<R>(cx)?;
    // The list's own scoped query, so the tile counts exactly the rows the
    // list would page through.
    let count = match scoped_query::<R>(cx) {
        Ok(query) => query.count().exec(&mut db(cx)).await.ok(),
        Err(_) => None,
    };
    Some(Stat {
        label: R::navigation_label(),
        url,
        icon: glyph,
        count,
    })
}

fn stat_card(cx: &Cx, stat: Stat) -> BoxView<'_> {
    let count = stat
        .count
        .map_or_else(|| "—".to_string(), |count| count.to_string());
    view! {
        cx =>
        <a
            href=(stat.url)
            class="group rounded-xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
            tablo_ui::card(
                attrs: topcoat::view::attributes! { class="transition-colors group-hover:bg-muted/40" },
                tablo_ui::card_content(
                    <div class="flex items-center justify-between gap-2">
                        <p class="text-sm font-medium text-muted-foreground">
                            (stat.label)
                        </p>
                        <span
                            class="text-muted-foreground [&>svg]:size-4"
                            aria-hidden="true"
                        >
                            icon(data: stat.icon)
                        </span>
                    </div>
                    <p
                        class="mt-2 text-3xl font-semibold tracking-tight text-foreground"
                    >
                        (count)
                    </p>
                )
            )
        </a>
    }
    .boxed()
}

impl Page for Dashboard {
    fn navigation() -> NavigationItem {
        NavigationItem::for_page::<Self>().icon(tablo_ui::icons::LAYOUT_DASHBOARD)
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        let stats = [
            stat::<UserResource>(cx, tablo_ui::icons::USERS).await,
            stat::<AuthorResource>(cx, tablo_ui::icons::PEN_LINE).await,
            stat::<PostResource>(cx, tablo_ui::icons::FILE_TEXT).await,
            stat::<CommentResource>(cx, tablo_ui::icons::MESSAGE_SQUARE).await,
        ];
        let cards: Vec<BoxView<'_>> = stats
            .into_iter()
            .flatten()
            .map(|stat| stat_card(cx, stat))
            .collect();
        Ok(view! {
            cx =>
            tablo_ui::page(
                tablo_ui::page_header(
                    tablo_ui::page_title("Dashboard")
                    tablo_ui::page_description(
                        "The blog's admin: its users, authors, posts, comments and media."
                    )
                )
                tablo_ui::page_content(
                    <div class="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
                        for card in cards {
                            (card)
                        }
                    </div>
                )
            )
        })
    }
}
