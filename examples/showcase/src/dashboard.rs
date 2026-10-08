//! The panel's home page, served at the panel prefix by `Panel::home`.

use tablo::{
    FieldErrors, HeaderAction, HeaderActions, NavigationItem, Page, Resource, can_list, db::db,
    header_actions, panel, scoped_query,
};
use topcoat::{
    Result,
    context::Cx,
    icon::{IconData, icon},
    view::{BoxView, View, ViewExt, view},
};

use crate::{
    app::{AuthorResource, CommentResource, PostResource, UserResource},
    models::Post,
};

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
            tablo::ui::card(
                attrs: topcoat::view::attributes! { class="transition-colors group-hover:bg-muted/40" },
                tablo::ui::card_content(
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

/// What featuring by tag asks for: the tag.
#[derive(tablo::ActionInput)]
pub struct FeatureTag {
    #[form(label = "Tag", placeholder = "rust")]
    pub tag: String,
}

/// Features every post carrying a tag, from the dashboard's header, asking which tag first.
pub struct FeatureTagged;

impl HeaderAction for FeatureTagged {
    type Input = FeatureTag;
    const NAME: &'static str = "feature-tagged";

    /// The dashboard admits every signed-in user; featuring takes the post list.
    fn can_run(cx: &Cx) -> bool {
        can_list::<PostResource>(cx)
    }

    fn validate_input(_cx: &Cx, input: &FeatureTag) -> FieldErrors {
        let mut errors = FieldErrors::new();
        if input.tag.contains(',') {
            errors.add("tag", "One tag, without commas");
        }
        errors
    }

    async fn run(cx: &Cx, input: FeatureTag, ex: &mut dyn toasty::Executor) -> Result<()> {
        let tag = input.tag.trim();
        let posts = scoped_query::<PostResource>(cx)?.exec(&mut *ex).await?;
        for post in posts {
            if post.tags.split(',').any(|t| t.trim() == tag) {
                Post::filter(Post::fields().id().eq(post.id))
                    .update()
                    .featured(true)
                    .exec(&mut *ex)
                    .await?;
            }
        }
        Ok(())
    }
}

impl Page for Dashboard {
    fn navigation() -> NavigationItem {
        NavigationItem::for_page::<Self>().icon(tablo::ui::icons::LAYOUT_DASHBOARD)
    }

    fn header_actions() -> HeaderActions {
        HeaderActions::new().add::<FeatureTagged>()
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
            tablo::ui::page(
                tablo::ui::page_header(
                    tablo::ui::page_title("Dashboard")
                    tablo::ui::page_description(
                        "The blog's admin: its users, authors, posts, comments and media."
                    )
                    tablo::ui::page_actions((header_actions::<Self>(cx)))
                )
                tablo::ui::page_content(
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
