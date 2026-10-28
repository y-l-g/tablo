//! The Actions chapter's snippets.

use tablo::{HeaderAction, Places, prelude::*};
use topcoat::{Result, context::Cx};

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

    async fn run(
        _cx: &Cx,
        posts: &[Post],
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> Result<tablo::Outcome, tablo::WriteError> {
        Post::filter(filter_keys(posts))
            .update()
            .status(PostStatus::Published)
            .exec(ex)
            .await?;
        Ok(tablo::Outcome::done())
    }
}
// ANCHOR_END: publish-action

// ANCHOR: input-action
/// What changing a post's status asks for.
#[derive(FormInput)]
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

    fn label() -> Text {
        "Change status".into()
    }

    fn validate_input(_cx: &Cx, change: &StatusChange) -> FieldErrors<StatusChangeField> {
        let mut errors = FieldErrors::new();
        if change.featured && change.status == PostStatus::Draft {
            errors.add(
                StatusChangeField::Featured,
                "Only a published post is featured",
            );
        }
        errors
    }

    async fn run(
        _cx: &Cx,
        posts: &[Post],
        change: StatusChange,
        ex: &mut dyn toasty::Executor,
    ) -> Result<tablo::Outcome, tablo::WriteError> {
        Post::filter(filter_keys(posts))
            .update()
            .status(change.status)
            .featured(change.featured)
            .exec(ex)
            .await?;
        Ok(tablo::Outcome::done())
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

    async fn run(
        cx: &Cx,
        _: (),
        ex: &mut dyn toasty::Executor,
    ) -> Result<tablo::Outcome, tablo::WriteError> {
        // No record is loaded: the action scopes its own query to the request's tenant.
        let drafts = scoped_query::<PostResource>(cx)?
            .filter(Post::fields().status().eq(PostStatus::Draft))
            .exec(&mut *ex)
            .await?;
        // The tenant's drafts are unbounded: one statement per chunk keeps each one bounded.
        for chunk in drafts.chunks(500) {
            Post::filter(filter_keys(chunk))
                .update()
                .status(PostStatus::Published)
                .exec(&mut *ex)
                .await?;
        }
        Ok(tablo::Outcome::done())
    }
}
// ANCHOR_END: header-action

// ANCHOR: page-header-actions
pub struct MaintenancePage;

impl Page for MaintenancePage {
    fn declare() -> PageDef {
        PageDef::new().header_action::<PublishDrafts>()
    }
}
// ANCHOR_END: page-header-actions
