//! Custom actions: the [`Action`] trait, and the [`Actions`] list a
//! [`ResourceDef`](super::ResourceDef) declares them in.

mod input;

use std::{any::Any, collections::HashMap, future::Future, pin::Pin};

pub use input::ActionInput;
#[doc(hidden)]
pub use input::required_input;
pub(crate) use input::{RESERVED_KEYS, SUBMITTED_KEY};
use topcoat::{Result, context::Cx};

use super::{Mounted, Resource};
use crate::{
    form::{FieldError, FieldErrors},
    naming::sentence_case,
    policy::Ability,
    schema::Schema,
};

/// A mutation beyond create, update and delete: "publish", "archive",
/// "resend the invite".
///
/// An action runs on one record, from a button in its table row or in the header of its detail or
/// edit page, or on the selection, from the bulk bar: [`PLACES`](Self::PLACES) says which.
/// [`ResourceDef::action`](super::ResourceDef::action) declares it:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     title: String,
/// #     status: String,
/// # }
/// # use tablo_core::{Action, NoForm, Resource};
/// # use toasty::Executor;
/// # use topcoat::{Result, context::Cx};
/// # struct PostResource;
/// #
/// # impl Resource for PostResource {
/// #     type Model = Post;
/// #     type Form = NoForm<Post>;
/// # }
/// struct Publish;
///
/// impl Action<PostResource> for Publish {
///     type Input = ();
///     const NAME: &'static str = "publish";
///
///     fn can_run(_cx: &Cx, post: &Post) -> bool {
///         post.status != "published"
///     }
///
///     async fn run(_cx: &Cx, posts: &[Post], _: (), ex: &mut dyn Executor) -> Result<()> {
///         for post in posts {
///             Post::filter(Post::fields().id().eq(post.id))
///                 .update()
///                 .status("published".to_string())
///                 .exec(&mut *ex)
///                 .await?;
///         }
///         Ok(())
///     }
/// }
/// ```
///
/// The framework owns everything around [`run`](Self::run), as it does for
/// a delete:
///
/// - the route, `{list}/{key}/-/actions/{NAME}` for one record and `{list}/-/actions/{NAME}` for
///   the selection, and its CSRF check;
/// - the transaction: the records are loaded through [`scoped_query`](super::scoped_query) inside
///   it, `run` writes through the same executor, and an error rolls everything back;
/// - the policy: [`Ability::RunAny`] with the action's `NAME`, asked before the body is read, then
///   [`Ability::View`] and [`Ability::Run`] on every loaded record before `run`. A record `View`
///   refuses fails the whole POST with 403;
/// - the refusal: a row that `Run` or [`can_run`](Self::can_run) refuses answers 403, and a bulk
///   selection runs the records that pass both and reports the refused count as skipped. A
///   selection that passes on none writes nothing and answers with an error notification;
/// - [`Resource::after_commit`] with [`Mutation::Action`](super::Mutation::Action) once the
///   transaction commits, and the success notification.
///
/// An action that asks for an [`Input`](Self::Input) asks for it first: its button opens the
/// input's form in a dialog over the page, or, without JavaScript, as a page the action's route
/// renders after the same checks. The submit runs the action with the parsed value: it parses the
/// input, asks [`validate_input`](Self::validate_input) and checks its choices before the
/// transaction opens, then re-checks a relationship choice inside it. A refused value renders the
/// input as a page with its errors and writes nothing.
///
/// A run from a detail or edit page lands back on that page; one from a row or the bulk bar lands
/// on the list.
///
/// No place renders a button for an action the policy refuses `RunAny`, and no row or page renders
/// one for a record that fails `View`, `Run` or `can_run`. A row that no bulk action and no delete
/// allows renders no checkbox.
pub trait Action<R: Resource>: 'static {
    /// What the action asks for before it runs: `()` for nothing, or an
    /// [`ActionInput`](derive@crate::ActionInput) struct, such as a rejection's reason.
    type Input: ActionInput;

    /// The action's URL segment, distinct among the resource's actions.
    ///
    /// [`ResourceDef::action`](super::ResourceDef::action) refuses to compile a name that is not
    /// a single path segment: empty, `.` or `..`, or holding whitespace, a control character, a
    /// quote, a backslash or one of `/ ? # % & = { } ( )`.
    /// [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel) refuses a name that
    /// another action of the resource shares.
    ///
    /// ```rust,compile_fail
    /// # use tablo_core::{Action, NoForm, Resource, ResourceDef};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// # }
    /// struct Archive;
    ///
    /// impl Action<PostResource> for Archive {
    ///     type Input = ();
    ///     const NAME: &'static str = "archive/all";
    /// #   fn can_run(_: &Cx, _: &Post) -> bool { true }
    /// #   async fn run(_: &Cx, _: &[Post], _: (), _: &mut dyn toasty::Executor) -> Result<()> { Ok(()) }
    /// }
    ///
    /// let def = ResourceDef::<PostResource>::new().action::<Archive>();
    /// ```
    const NAME: &'static str;

    /// Where the action's button renders. Defaults to [`Places::ALL`]: each row, the detail and
    /// edit pages' headers, and the bulk bar.
    ///
    /// It decides where the button shows, not who may run it: the record route serves an action
    /// placed on any of [`Places::RECORD`], and the bulk route one placed on
    /// [`Places::BULK`].
    const PLACES: Places = Places::ALL;

    /// Whether the action asks first through a confirmation dialog sharing the
    /// delete dialog's mechanism and destructive wording. Defaults to `false`.
    ///
    /// An action with input confirms in its input dialog, or on its input page, instead, which
    /// say the action cannot be undone and whose submit renders destructive. An unconfirmed POST
    /// that would write answers 400.
    const CONFIRM: bool = false;

    /// The button text. Defaults to [`NAME`](Self::NAME) in sentence case: `"publish"` reads
    /// "Publish", and `"send-invite"` reads "Send invite".
    fn label(_cx: &Cx) -> String {
        sentence_case(Self::NAME)
    }

    /// Whether `record`'s state lets the action run on it: a published post refuses "publish".
    /// Defaults to `true`.
    ///
    /// It sees no policy, so it decides no authorization: the resource's policy answers
    /// [`Ability::Run`] for that, and a panel that mounts the resource with another policy
    /// changes who may run the action.
    fn can_run(_cx: &Cx, _record: &R::Model) -> bool {
        true
    }

    /// Refuse an input [`run`](Self::run) should not receive, such as a reason too short to
    /// act on: each error names an input field's key and renders under its control. Defaults to
    /// none.
    ///
    /// It runs after the input parses and before the transaction opens, so it sees no record.
    fn validate_input(_cx: &Cx, _input: &Self::Input) -> FieldErrors {
        FieldErrors::new()
    }

    /// Perform the action on `records`, the records of the row or selection that
    /// passed [`Ability::Run`] and [`can_run`](Self::can_run), with the parsed `input`, through
    /// the framework's transaction `ex`.
    fn run(
        cx: &Cx,
        records: &[R::Model],
        input: Self::Input,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = Result<()>> + Send;

    /// The success notification after a commit. Defaults to the label and
    /// the record count: `"Publish: 3 records"`. A bulk run the action refused
    /// on some records appends their count out of the selection:
    /// `"Publish: 3 records (2 of 5 skipped)"`.
    fn success(cx: &Cx, count: usize) -> String {
        let noun = if count == 1 { "record" } else { "records" };
        format!("{}: {count} {noun}", Self::label(cx))
    }
}

/// Where an [`Action`]'s button renders: [`Action::PLACES`].
///
/// ```rust
/// # use tablo_core::Places;
/// // On the detail page and the bulk bar, not on each row or the edit page.
/// const PLACES: Places = Places::DETAIL.with(Places::BULK);
/// assert!(PLACES.contains(Places::BULK));
/// assert!(!PLACES.contains(Places::ROW));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Places(u8);

impl Places {
    /// A button in each table row, the related tables' included.
    pub const ROW: Self = Self(1);
    /// A button in the detail page's header.
    pub const DETAIL: Self = Self(1 << 1);
    /// A button in the edit page's header.
    pub const EDIT: Self = Self(1 << 2);
    /// An entry in the bulk bar, run on the selection.
    pub const BULK: Self = Self(1 << 3);
    /// Every place that shows one record: [`ROW`](Self::ROW), [`DETAIL`](Self::DETAIL) and
    /// [`EDIT`](Self::EDIT).
    pub const RECORD: Self = Self(Self::ROW.0 | Self::DETAIL.0 | Self::EDIT.0);
    /// Every place.
    pub const ALL: Self = Self(Self::RECORD.0 | Self::BULK.0);

    /// These places and `other`'s.
    #[must_use]
    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether every place of `other` is one of these.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether any place of `other` is one of these.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

impl<M> Ability<'_, M> {
    /// Whether the ability is [`RunAny`](Ability::RunAny) or [`Run`](Ability::Run) for the action
    /// `A`.
    ///
    /// It compares against `A::NAME`, so renaming the action keeps the policy matching it, and a
    /// type that is not an action of the resource does not compile. `R` is inferred when `A` is an
    /// action of one resource.
    ///
    /// ```rust
    /// # use tablo_core::{Ability, Action, NoForm, Resource};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, published: bool }
    /// # struct PostResource;
    /// # impl Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = NoForm<Post>;
    /// # }
    /// # struct Publish;
    /// # impl Action<PostResource> for Publish {
    /// #     type Input = ();
    /// #     const NAME: &'static str = "publish";
    /// #     async fn run(_: &Cx, _: &[Post], _: (), _: &mut dyn toasty::Executor) -> Result<()> { Ok(()) }
    /// # }
    /// # fn is_editor(_cx: &Cx) -> bool { true }
    /// fn post_policy(cx: &Cx, ability: Ability<'_, Post>) -> bool {
    ///     match ability {
    ///         Ability::Run { record, .. } if ability.is_action::<Publish, _>() => !record.published,
    ///         Ability::ViewAny | Ability::View(_) => true,
    ///         _ => is_editor(cx),
    ///     }
    /// }
    /// ```
    pub fn is_action<A, R>(self) -> bool
    where
        A: Action<R>,
        R: Resource<Model = M>,
    {
        matches!(self, Ability::RunAny { action } | Ability::Run { action, .. } if action == A::NAME)
    }
}

/// What an erased action's `run` returns.
pub(crate) type ActionFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

/// A parsed [`Action::Input`] with its type erased; `run` downcasts it back.
pub(crate) type ErasedInput = Box<dyn Any + Send>;

/// What an erased input parse returns.
pub(crate) type InputResult = std::result::Result<ErasedInput, Vec<FieldError>>;

/// What an action asks for before it runs, its type erased: the input's schema and its parse.
#[derive(Clone, Copy)]
pub(crate) struct InputSpec {
    /// The input form's schema: empty for a delete and an action asking for nothing.
    pub(crate) schema: fn() -> Schema,
    /// Whether [`schema`](Self::schema) declares a field, so the action asks for it on a page.
    pub(crate) takes_input: bool,
    /// Parses and validates the input from a submission, as the value `run` takes.
    pub(crate) parse: fn(&Cx, &HashMap<String, String>) -> InputResult,
}

impl InputSpec {
    /// The input of type `I`, parsed and validated by `parse`.
    pub(crate) fn of<I: ActionInput>(
        parse: fn(&Cx, &HashMap<String, String>) -> InputResult,
    ) -> Self {
        Self {
            schema: I::schema,
            takes_input: !I::schema().is_empty(),
            parse,
        }
    }

    /// No input: a delete's.
    pub(crate) fn none() -> Self {
        Self {
            schema: Schema::empty,
            takes_input: false,
            parse: parse_nothing,
        }
    }
}

/// The actions a [`Resource`] declares, in button order.
pub(crate) struct Actions<R: Resource> {
    entries: Vec<ActionEntry<R>>,
}

impl<R: Resource> Default for Actions<R> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<R: Resource> std::fmt::Debug for Actions<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.entries.iter().map(|e| e.name))
            .finish()
    }
}

impl<R: Resource> Actions<R> {
    /// Appends the action `A`, failing to compile when `A::NAME` is not a single path segment.
    pub(crate) fn add<A: Action<R>>(mut self) -> Self {
        const {
            assert!(
                crate::declaration::segment_fault(A::NAME).is_none(),
                "`Action::NAME` must be a single path segment"
            );
        }
        self.entries.push(ActionEntry {
            name: A::NAME,
            label: A::label,
            places: A::PLACES,
            resource_wide: Ability::RunAny { action: A::NAME },
            can_run: can_run_erased::<R, A>,
            input: InputSpec::of::<A::Input>(parse_input_erased::<R, A>),
            run: run_erased::<R, A>,
            success: A::success,
            acted: super::Committed::acted::<R, A>,
            failure: "run the action",
            confirm: A::CONFIRM,
        });
        self
    }

    /// The declared actions, in order.
    pub(crate) fn entries(&self) -> &[ActionEntry<R>] {
        &self.entries
    }

    /// The action named `name`.
    pub(crate) fn find(&self, name: &str) -> Option<&ActionEntry<R>> {
        self.entries.iter().find(|e| e.name == name)
    }
}

/// One mutation of a record or a selection with its type erased: a declared action, or the
/// built-in delete. The panel runs every one through the same pipeline.
pub(crate) struct ActionEntry<R: Resource> {
    pub(crate) name: &'static str,
    pub(crate) label: fn(&Cx) -> String,
    pub(crate) places: Places,
    /// The resource-wide ability checked before the body is read.
    pub(crate) resource_wide: Ability<'static, R::Model>,
    /// Whether the mutation may write `record`, which already passed `View`.
    pub(crate) can_run: fn(&Mounted<R>, &Cx, &R::Model) -> bool,
    pub(crate) input: InputSpec,
    pub(crate) run: for<'a> fn(
        &'a Cx,
        &'a [R::Model],
        ErasedInput,
        &'a mut dyn toasty::Executor,
    ) -> ActionFuture<'a>,
    pub(crate) success: fn(&Cx, usize) -> String,
    pub(crate) acted: fn(Vec<R::Model>) -> super::Committed<R::Model>,
    /// The failure toast's wording: "Couldn't {failure}".
    pub(crate) failure: &'static str,
    pub(crate) confirm: bool,
}

impl<R: Resource> Clone for ActionEntry<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: Resource> Copy for ActionEntry<R> {}

impl<R: Resource> ActionEntry<R> {
    /// One record's Delete, from its row or its detail or edit page: [`Resource::delete_record`],
    /// on a record [`Ability::Delete`] allows.
    pub(crate) fn delete() -> Self {
        Self::deleting(false)
    }

    /// The bulk bar's Delete: [`Resource::bulk_delete_records`], on the selected records
    /// [`Ability::Delete`] allows.
    pub(crate) fn bulk_delete() -> Self {
        Self::deleting(true)
    }

    fn deleting(bulk: bool) -> Self {
        Self {
            name: "delete",
            label: |_| "Delete".to_string(),
            places: if bulk { Places::BULK } else { Places::RECORD },
            resource_wide: Ability::DeleteAny,
            can_run: |resource, cx, record| resource.can(cx, Ability::Delete(record)),
            input: InputSpec::none(),
            run: if bulk {
                bulk_delete_erased::<R>
            } else {
                delete_erased::<R>
            },
            success: if bulk {
                |_, _| "Bulk deleted".to_string()
            } else {
                |_, _| "Deleted".to_string()
            },
            acted: super::Committed::deleted,
            failure: if bulk {
                "delete the selected rows"
            } else {
                "delete the record"
            },
            confirm: true,
        }
    }
}

/// The mounted policy's [`Ability::Run`] and [`Action::can_run`] behind a function pointer.
fn can_run_erased<R: Resource, A: Action<R>>(
    resource: &Mounted<R>,
    cx: &Cx,
    record: &R::Model,
) -> bool {
    resource.can(
        cx,
        Ability::Run {
            action: A::NAME,
            record,
        },
    ) && A::can_run(cx, record)
}

/// [`ActionInput::parse`], then [`Action::validate_input`], behind a function pointer.
fn parse_input_erased<R: Resource, A: Action<R>>(
    cx: &Cx,
    values: &HashMap<String, String>,
) -> InputResult {
    parse_validated(cx, values, A::validate_input)
}

/// `I`'s [`ActionInput::parse`], then `validate`.
pub(crate) fn parse_validated<I: ActionInput>(
    cx: &Cx,
    values: &HashMap<String, String>,
    validate: fn(&Cx, &I) -> FieldErrors,
) -> InputResult {
    let input = I::parse(cx, values)?;
    let refused: Vec<FieldError> = validate(cx, &input).into_iter().collect();
    if !refused.is_empty() {
        return Err(refused);
    }
    Ok(Box::new(input))
}

/// A delete's input: nothing.
fn parse_nothing(_cx: &Cx, _values: &HashMap<String, String>) -> InputResult {
    Ok(Box::new(()))
}

/// [`Action::run`] behind a function pointer.
fn run_erased<'a, R: Resource, A: Action<R>>(
    cx: &'a Cx,
    records: &'a [R::Model],
    input: ErasedInput,
    ex: &'a mut dyn toasty::Executor,
) -> ActionFuture<'a> {
    let input = *input
        .downcast::<A::Input>()
        .expect("the pipeline parses the input `run` takes");
    Box::pin(A::run(cx, records, input, ex))
}

/// [`Resource::delete_record`] on each of `records`, behind a function pointer.
fn delete_erased<'a, R: Resource>(
    cx: &'a Cx,
    records: &'a [R::Model],
    _input: ErasedInput,
    ex: &'a mut dyn toasty::Executor,
) -> ActionFuture<'a> {
    Box::pin(async move {
        for record in records {
            R::delete_record(cx, record, &mut *ex).await?;
        }
        Ok(())
    })
}

/// [`Resource::bulk_delete_records`] behind a function pointer.
fn bulk_delete_erased<'a, R: Resource>(
    cx: &'a Cx,
    records: &'a [R::Model],
    _input: ErasedInput,
    ex: &'a mut dyn toasty::Executor,
) -> ActionFuture<'a> {
    Box::pin(R::bulk_delete_records(cx, records, ex))
}
