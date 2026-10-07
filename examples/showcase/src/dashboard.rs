//! The panel's home page, served at the panel prefix by `Panel::home`.

use tablo_core::{NavigationItem, Page, Resource, can_list, db::db, panel, scoped_query};
use topcoat::{
    Result,
    context::Cx,
    icon::{IconData, icon},
    view::{BoxView, View, ViewExt, view},
};

use crate::app::{AuthorResource, CommentResource, PostResource, UserResource};

/// The page the panel serves at its prefix.
pub struct Dashboard;

/// One resource's tile: its sidebar label, list and icon, and how many records the caller can
/// see there.
struct Stat {
    label: String,
    url: String,
    icon: Option<IconData>,
    /// `None` when the count could not be read: the tile shows a dash rather
    /// than a wrong zero.
    count: Option<u64>,
}

/// `R`'s tile, or `None` when the caller may not open `R`'s list, so the
/// dashboard links to no list that would answer 403, or when the panel does
/// not register `R`.
async fn stat<R: Resource>(cx: &Cx) -> Option<Stat> {
    if !can_list::<R>(cx) {
        return None;
    }
    let item = panel::navigation::<R>(cx)?;
    let url = panel::url::resource::<R>(cx)?;
    let count = match scoped_query::<R>(cx) {
        Ok(query) => query.count().exec(&mut db(cx)).await.ok(),
        Err(_) => None,
    };
    Some(Stat {
        url,
        label: item.label,
        icon: item.icon,
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
            (topcoat::runtime::link_attrs(
                cx,
                stat.url,
                topcoat::runtime::prefetch_mode(cx),
            ))
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
                            if let Some(glyph) = stat.icon {
                                icon(data: glyph)
                            }
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
            stat::<UserResource>(cx).await,
            stat::<AuthorResource>(cx).await,
            stat::<PostResource>(cx).await,
            stat::<CommentResource>(cx).await,
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
