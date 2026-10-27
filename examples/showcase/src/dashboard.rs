//! The panel's home page, served at the panel prefix by `Panel::home`.

use jiff::tz::TimeZone;
use tablo::{
    Ability, Chart, Color, FieldErrors, HeaderAction, Page, PageDef, RecordTable, Series, Stat,
    Widget, WidgetDef, can_list, db::db, header_action_buttons, scoped_query,
};
use topcoat::{
    Result,
    context::Cx,
    view::{View, view},
};

use crate::{
    app::{AuthorResource, CommentResource, PostResource, UserResource},
    models::{Post, PostStatus},
};

/// The page the panel serves at its prefix.
pub struct Dashboard;

/// The record counts the caller can see, one tile per resource whose list they may open.
pub struct Overview;

impl Widget for Overview {
    fn declare() -> WidgetDef {
        WidgetDef::new().full_width()
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        let posts = match Stat::records::<PostResource>(cx).await? {
            Some(posts) => {
                let drafts = scoped_query::<PostResource>(cx)?
                    .filter(Post::fields().status().eq(PostStatus::Draft))
                    .count()
                    .exec(&mut db(cx)?)
                    .await?;
                Some(posts.detail(format!("{drafts} in draft")))
            }
            None => None,
        };
        let tiles = [
            Stat::records::<UserResource>(cx).await?,
            Stat::records::<AuthorResource>(cx).await?,
            posts,
            Stat::records::<CommentResource>(cx).await?,
        ];
        Ok(tablo::stats(cx, tiles.into_iter().flatten()))
    }
}

/// How many months [`PostsPerMonth`] spans.
const CHART_MONTHS: i64 = 6;

/// The posts the caller can see, published and in draft, per month over the six months up to the
/// newest one's. Only a caller who may open the post list sees it.
pub struct PostsPerMonth;

impl Widget for PostsPerMonth {
    fn declare() -> WidgetDef {
        WidgetDef::new().access(can_list::<PostResource>)
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        let posts = scoped_query::<PostResource>(cx)?.exec(&mut db(cx)?).await?;
        let month = |post: &Post| {
            post.created_at
                .to_zoned(TimeZone::UTC)
                .date()
                .first_of_month()
        };
        let months: Vec<jiff::civil::Date> =
            posts.iter().map(month).max().map_or_else(Vec::new, |last| {
                (0..CHART_MONTHS)
                    .rev()
                    .map(|back| last.saturating_sub(jiff::Span::new().months(back)))
                    .collect()
            });
        let count = |status: PostStatus| -> Vec<f64> {
            months
                .iter()
                .map(|&first| {
                    posts
                        .iter()
                        .filter(|post| post.status == status && month(post) == first)
                        .count() as f64
                })
                .collect()
        };
        let period = months.last().map_or_else(
            || "By the month each was written".to_string(),
            |last| {
                format!(
                    "By the month each was written, up to {}",
                    last.strftime("%B %Y")
                )
            },
        );
        let chart = Chart::bar(months.iter().map(|month| month.strftime("%b").to_string()))
            .heading("Posts per month")
            .description(period)
            .series(Series::new("Published", count(PostStatus::Published)).color(Color::Primary))
            .series(Series::new("Draft", count(PostStatus::Draft)).color(Color::Gray));
        Ok(tablo::chart(cx, chart))
    }
}

/// The newest posts the caller can see, under the post list's own columns, each linking to its
/// page, with a link to the full list. Only a caller who may open the post list sees it.
pub struct LatestPosts;

impl Widget for LatestPosts {
    fn declare() -> WidgetDef {
        WidgetDef::new().access(can_list::<PostResource>)
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        let table = RecordTable::new()
            .heading("Latest posts")
            .columns(["title", "author", "status"])
            .order_by(Post::fields().created_at().desc());
        tablo::record_table::<PostResource>(cx, table).await
    }
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
            .widget::<Overview>()
            .widget::<PostsPerMonth>()
            .widget::<LatestPosts>()
    }

    async fn render(cx: &Cx) -> Result<impl View> {
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
                tablo::ui::page_content((tablo::widgets::<Self>(cx)))
            )
        })
    }
}
