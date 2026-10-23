//! The panel's home page, served at the panel prefix by `Panel::home`.

use tablo::{
    Ability, FieldErrors, HeaderAction, Page, PageDef, Resource, can_list, db::db,
    header_action_buttons, scoped_query,
};
use toasty::stmt::Include;
use topcoat::{
    Result,
    context::Cx,
    view::{BoxView, View, ViewExt, view},
};

use crate::{
    app::{AuthorResource, CommentResource, PostResource, UserResource},
    models::{Author, Post, PostStatus},
};

/// The page the panel serves at its prefix.
pub struct Dashboard;

/// `R`'s tile: its sidebar label, icon and list, and how many records the caller can see there,
/// or `None` when the caller may not open `R`'s list, so the dashboard links to no list that
/// would answer 403, or when the panel does not register `R`. A count the database fails to read
/// shows as a dash rather than a wrong zero.
async fn stat<R: Resource>(cx: &Cx, detail: String) -> Option<BoxView<'_>> {
    if !can_list::<R>(cx) {
        return None;
    }
    let item = tablo::navigation_item::<R>(cx)?;
    let url = tablo::url::resource::<R>(cx)?;
    let count = match scoped_query::<R>(cx) {
        Ok(query) => query.count().exec(&mut db(cx)).await.ok(),
        Err(_) => None,
    };
    let value = count.map_or_else(|| "—".to_string(), |count| count.to_string());
    Some(
        view! {
            cx =>
            tablo::ui::stat(
                label: item.label.localize(cx),
                value: value,
                detail: detail,
                icon: item.icon,
                href: url
            )
        }
        .boxed(),
    )
}

/// The newest posts the caller can see, each linking to its page, with a link to the full list.
async fn latest_posts(cx: &Cx) -> Option<BoxView<'_>> {
    if !can_list::<PostResource>(cx) {
        return None;
    }
    let all = tablo::url::resource::<PostResource>(cx)?;
    let author: Include<Post, Author> = Post::fields().author().into();
    let posts = scoped_query::<PostResource>(cx)
        .ok()?
        .order_by(Post::fields().created_at().desc())
        .limit(5)
        .include(author)
        .exec(&mut db(cx))
        .await
        .ok()?;
    let rows: Vec<BoxView<'_>> = posts
        .iter()
        .map(|post| {
            let url = tablo::url::record::<PostResource>(cx, post);
            let byline = format!(
                "{} · {}",
                post.author.get().name,
                post.created_at.strftime("%b %-d, %Y")
            );
            let status = match post.status {
                PostStatus::Draft => "Draft",
                PostStatus::Published => "Published",
            };
            let title = post.title.clone();
            view! {
                cx =>
                <li class="flex items-center gap-4 px-6 py-3">
                    <div class="min-w-0 flex-1">
                        if let Some(url) = url {
                            <a
                                href=(url)
                                class="block truncate text-sm font-medium hover:underline"
                            >
                                (title)
                            </a>
                        } else {
                            <p class="truncate text-sm font-medium">(title)</p>
                        }
                        <p class="truncate text-sm text-muted-foreground">(byline)</p>
                    </div>
                    tablo::ui::badge(
                        variant: tablo::ui::BadgeVariant::Outline,
                        (status)
                    )
                </li>
            }
            .boxed()
        })
        .collect();
    Some(
        view! {
            cx =>
            <section
                class="rounded-xl border border-border bg-card text-card-foreground shadow-xs"
            >
                <div
                    class="flex items-center justify-between border-b border-border px-6 py-4"
                >
                    <h2 class="text-base font-semibold">"Latest posts"</h2>
                    <a
                        href=(all)
                        class="text-sm text-muted-foreground hover:text-foreground"
                    >
                        "View all"
                    </a>
                </div>
                <ul class="divide-y divide-border">
                    for row in rows {
                        (row)
                    }
                </ul>
            </section>
        }
        .boxed(),
    )
}

/// What featuring by tag asks for: the tag.
#[derive(tablo::FormInput)]
pub struct FeatureTag {
    #[form(label = "Tag", placeholder = "rust")]
    pub tag: String,
}

/// Features every post carrying a tag, from the dashboard's header, asking which tag first.
pub struct FeatureTagged;

impl HeaderAction for FeatureTagged {
    type Input = FeatureTag;
    const NAME: &'static str = "feature-tagged";

    /// The dashboard admits every signed-in user and has no policy: featuring asks the post
    /// resource's.
    fn can_run(cx: &Cx) -> bool {
        tablo::can::<PostResource>(cx, Ability::RunHeader { action: Self::NAME })
    }

    fn validate_input(_cx: &Cx, input: &FeatureTag) -> FieldErrors {
        let mut errors = FieldErrors::new();
        if input.tag.contains(',') {
            errors.add("tag", "One tag, without commas");
        }
        errors
    }

    async fn run(
        cx: &Cx,
        input: FeatureTag,
        ex: &mut dyn toasty::Executor,
    ) -> Result<tablo::Outcome, tablo::WriteError> {
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
        Ok(tablo::Outcome::done())
    }
}

impl Page for Dashboard {
    fn declare() -> PageDef {
        PageDef::new()
            .icon(tablo::ui::icons::LAYOUT_DASHBOARD)
            .header_action::<FeatureTagged>()
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        let drafts = scoped_query::<PostResource>(cx)?
            .filter(Post::fields().status().eq(PostStatus::Draft))
            .count()
            .exec(&mut db(cx))
            .await
            .map_or_else(|_| String::new(), |drafts| format!("{drafts} in draft"));
        let stats: Vec<BoxView<'_>> = [
            stat::<UserResource>(cx, String::new()).await,
            stat::<AuthorResource>(cx, String::new()).await,
            stat::<PostResource>(cx, drafts).await,
            stat::<CommentResource>(cx, String::new()).await,
        ]
        .into_iter()
        .flatten()
        .collect();
        let latest = latest_posts(cx).await;
        Ok(view! {
            cx =>
            tablo::ui::page(
                tablo::ui::page_header(
                    tablo::ui::page_title("Dashboard")
                    tablo::ui::page_description(
                        "The blog's admin: its users, authors, posts, comments and media."
                    )
                    tablo::ui::page_actions((header_action_buttons::<Self>(cx)))
                )
                tablo::ui::page_content(
                    <div class="grid grid-cols-2 gap-4 xl:grid-cols-4">
                        for tile in stats {
                            (tile)
                        }
                    </div>
                    if let Some(latest) = latest {
                        (latest)
                    }
                )
            )
        })
    }
}
