//! The Actions chapter's snippets.

use tablo::{HeaderAction, HeaderActions, Places, header_action_buttons, prelude::*};
use topcoat::{Result, context::Cx, view::*};

use crate::{
    models::{Post, PostStatus},
    resources::PostResource,
};

// ANCHOR: publish-action
pub(crate) struct Publish;

impl Action<PostResource> for Publish {
    type Input = ();
    const NAME: &'static str = "publish";

    fn can_run(_cx: &Cx, post: &Post) -> bool {
        post.status != PostStatus::Published
    }

    async fn run(_cx: &Cx, posts: &[Post], _: (), ex: &mut dyn toasty::Executor) -> Result<()> {
        for post in posts {
            Post::filter(Post::fields().id().eq(post.id))
                .update()
                .status(PostStatus::Published)
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}
// ANCHOR_END: publish-action

// ANCHOR: input-action
/// What changing a post's status asks for.
#[derive(ActionInput)]
pub(crate) struct StatusChange {
    #[form(options)]
    pub status: PostStatus,
    #[form(label = "Feature on the home page")]
    pub featured: bool,
}

pub(crate) struct ChangeStatus;

impl Action<PostResource> for ChangeStatus {
    type Input = StatusChange;
    const NAME: &'static str = "change-status";
    // ANCHOR: places
    // On the detail page and the bulk bar, not on each row or the edit page.
    const PLACES: Places = Places::DETAIL.with(Places::BULK);
    // ANCHOR_END: places

    fn label(_cx: &Cx) -> String {
        "Change status".to_string()
    }

    async fn run(
        _cx: &Cx,
        posts: &[Post],
        change: StatusChange,
        ex: &mut dyn toasty::Executor,
    ) -> Result<()> {
        for post in posts {
            Post::filter(Post::fields().id().eq(post.id))
                .update()
                .status(change.status)
                .featured(change.featured)
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}
// ANCHOR_END: input-action

// ANCHOR: header-action
/// Publishes every draft, from the post list's header.
pub(crate) struct PublishDrafts;

impl HeaderAction for PublishDrafts {
    type Input = ();
    const NAME: &'static str = "publish-drafts";
    const CONFIRM: bool = true;

    /// A page has no policy: the action asks the post resource's own, wherever it is declared.
    fn can_run(cx: &Cx) -> bool {
        can::<PostResource>(cx, Ability::RunHeader { action: Self::NAME })
    }

    async fn run(cx: &Cx, _: (), ex: &mut dyn toasty::Executor) -> Result<()> {
        // No record is loaded: the action scopes its own query to the request's tenant.
        let drafts = scoped_query::<PostResource>(cx)?
            .filter(Post::fields().status().eq(PostStatus::Draft))
            .exec(&mut *ex)
            .await?;
        for post in drafts {
            Post::filter(Post::fields().id().eq(post.id))
                .update()
                .status(PostStatus::Published)
                .exec(&mut *ex)
                .await?;
        }
        Ok(())
    }
}
// ANCHOR_END: header-action

// ANCHOR: page-header-actions
pub struct MaintenancePage;

impl Page for MaintenancePage {
    fn header_actions() -> HeaderActions {
        HeaderActions::new().add::<PublishDrafts>()
    }

    async fn render(cx: &Cx) -> Result<impl View> {
        Ok(view! {
            cx =>
            tablo::ui::page(
                tablo::ui::page_header(
                    tablo::ui::page_title("Maintenance")
                    // The buttons of the actions above the request may run.
                    tablo::ui::page_actions((header_action_buttons::<Self>(cx)))
                )
            )
        })
    }
}
// ANCHOR_END: page-header-actions
