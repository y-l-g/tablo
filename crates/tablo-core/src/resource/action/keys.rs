//! [`filter_keys`]: the predicate an action's `run` writes its records through in one statement.

use toasty::stmt::{Expr, IntoExpr};

use crate::toasty_compat::pk::pk_set_filter;

/// The predicate selecting each of `records` by its primary key, so that an action writes the
/// same change to all of them in one statement rather than one per record:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     title: String,
/// #     status: String,
/// # }
/// # use tablo_core::{Action, NoForm, Resource, filter_keys};
/// # use toasty::Executor;
/// # use topcoat::{Result, context::Cx};
/// # struct PostResource;
/// # impl Resource for PostResource {
/// #     type Model = Post;
/// #     type Form = NoForm<Post>;
/// # }
/// # struct Publish;
/// # impl Action<PostResource> for Publish {
/// #     type Input = ();
/// #     const NAME: &'static str = "publish";
/// async fn run(
///     _cx: &Cx,
///     posts: &[Post],
///     _: (),
///     ex: &mut dyn Executor,
/// ) -> Result<tablo_core::Outcome, tablo_core::WriteError<std::convert::Infallible>> {
///     Post::filter(filter_keys(posts))
///         .update()
///         .status("published".to_string())
///         .exec(ex)
///         .await?;
///     Ok(tablo_core::Outcome::done())
/// }
/// # }
/// ```
///
/// A single key selects through one `IN` list, a composite key through one equality per record,
/// and no record selects no row. The predicate scopes nothing itself: it selects exactly the
/// records given, which for an [`Action`](crate::Action) the pipeline loaded through the
/// tenant-scoped query and checked against the policy, no more than
/// [`Limits::bulk_records`](crate::Limits::bulk_records) of them.
///
/// The statement binds a value per key, which databases bound: SQLite takes 32,766 values, and
/// nests the equalities of composite keys no more than 1,000 deep. A set the caller loaded itself,
/// as a [`HeaderAction`](crate::HeaderAction) does, has no such bound: write it in chunks, a few
/// hundred records each.
pub fn filter_keys<M>(records: &[M]) -> Expr<bool>
where
    M: toasty::schema::Model + IntoExpr<M>,
{
    pk_set_filter(&records.iter().collect::<Vec<_>>())
}
