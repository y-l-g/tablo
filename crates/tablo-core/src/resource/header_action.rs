//! Actions on no record: the [`HeaderAction`] trait, and the [`HeaderActions`] a
//! [`Page`](crate::Page) or a [`ResourceDef`](super::ResourceDef) declares them in.

use std::future::Future;

use topcoat::{Result, context::Cx};

use super::{ErasedInput, InputResult, InputSpec, parse_validated};
use crate::{
    BoxFuture, FormInput,
    form::{FieldErrors, FormValues},
    naming::{derived_id, sentence_case},
    policy::Ability,
};

/// A mutation that acts on no record, run from a button in a page's header: "import posts",
/// "clear the cache", "recompute the totals".
///
/// A resource declares one with
/// [`ResourceDef::header_action`](crate::ResourceDef::header_action), and its list page renders
/// the button; a [`Page`](crate::Page) declares one with
/// [`PageDef::header_action`](crate::PageDef::header_action), and renders the buttons with
/// [`header_action_buttons`](crate::header_action_buttons):
///
/// ```rust
/// # use tablo_core::HeaderAction;
/// # use toasty::Executor;
/// # use topcoat::{Result, context::Cx};
/// struct RecountTags;
///
/// impl HeaderAction for RecountTags {
///     type Input = ();
///     const NAME: &'static str = "recount-tags";
///
///     async fn run(
///         _cx: &Cx,
///         _: (),
///         _ex: &mut dyn Executor,
///     ) -> Result<tablo_core::Outcome, tablo_core::WriteError> {
///         // Recount through `ex`, the framework's transaction.
///         Ok(tablo_core::Outcome::done())
///     }
/// }
/// ```
///
/// The framework owns everything around [`run`](Self::run), as it does for a record's
/// [`Action`](crate::Action):
///
/// - the route, `{url}/-/actions/{NAME}` under the list or page URL, and its CSRF check;
/// - who may run it: on a resource, the policy's [`Ability::ViewAny`] and [`Ability::RunHeader`]
///   with the action's `NAME`; on a page, [`PageDef::access`](crate::PageDef::access); on either,
///   then [`can_run`](Self::can_run). A refusal answers 403 and renders no button;
/// - the transaction: `run` writes through its executor, and an error rolls everything back;
/// - on a resource, the hooks, with [`Change::HeaderAction`](crate::Change::HeaderAction):
///   [`Resource::before_commit`](crate::Resource::before_commit) in the transaction after `run`,
///   then [`Resource::after_commit`](crate::Resource::after_commit) once it commits;
/// - the success notification, and the redirect back to the list as it was left, its search, sort
///   and filters kept, or to the page.
///
/// No record is loaded, so nothing scopes [`run`](Self::run) to the request's tenant: an action
/// over a tenant's records reads them through [`scoped_query`](crate::scoped_query).
///
/// An action that asks for an [`Input`](Self::Input) asks for it as a record's action does, in a
/// dialog or a page titled with its label, and runs on its submit.
pub trait HeaderAction: 'static {
    /// What the action asks for before it runs: `()` for nothing, or an
    /// [`FormInput`](derive@crate::FormInput) struct.
    type Input: FormInput;

    /// The action's URL segment, distinct among the actions of its resource or page.
    ///
    /// A name that is not a single path segment does not compile, as for
    /// [`Action::NAME`](crate::Action::NAME).
    const NAME: &'static str;

    /// Whether the action asks first through a confirmation dialog with destructive wording.
    /// Defaults to `false`.
    ///
    /// An action with input confirms in its input dialog, or on its input page, instead. An
    /// unconfirmed POST that would write answers 400.
    const CONFIRM: bool = false;

    /// The button text. Defaults to the message `action-{NAME}` of the panel's catalogs, else
    /// [`NAME`](Self::NAME) in sentence case: `"recount-tags"` reads "Recount tags".
    fn label(cx: &Cx) -> String {
        crate::Text::derived(derived_id("action", Self::NAME), sentence_case(Self::NAME))
            .localize(cx)
    }

    /// Whether the request may run the action, beyond the policy or the page's access: a
    /// feature flag, a role the policy does not model. Defaults to `true`.
    fn can_run(_cx: &Cx) -> bool {
        true
    }

    /// Refuse an input [`run`](Self::run) should not receive. Each error names an input field's
    /// key and renders under its control. Defaults to none. A key no control renders is a
    /// declaration error: the page would show no message.
    fn validate_input(_cx: &Cx, _input: &Self::Input) -> FieldErrors {
        FieldErrors::new()
    }

    /// Perform the action with the parsed `input`, through the framework's transaction `ex`,
    /// and say what the response does once it commits: [`Outcome::done`](crate::Outcome::done)
    /// for the [`success`](Self::success) notification, landing back on the list or page.
    ///
    /// Refuses the input with [`WriteError::Refused`](crate::WriteError::Refused), as
    /// [`Action::run`](crate::Action::run) does.
    fn run(
        cx: &Cx,
        input: Self::Input,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = Result<crate::Outcome, crate::WriteError>> + Send;

    /// The success notification after a commit. Defaults to the label: `"Recount tags: done"`.
    fn success(cx: &Cx) -> String {
        crate::t!(cx, "tablo-action-done", action = Self::label(cx))
    }
}

impl<M> Ability<'_, M> {
    /// Whether the ability is [`RunHeader`](Ability::RunHeader) for the header action `A`, which
    /// a resource's policy answers to let the request run it.
    ///
    /// ```rust
    /// # use tablo_core::{Ability, HeaderAction};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid }
    /// # struct RecountTags;
    /// # impl HeaderAction for RecountTags {
    /// #     type Input = ();
    /// #     const NAME: &'static str = "recount-tags";
    /// #     async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError> { Ok(tablo_core::Outcome::done()) }
    /// # }
    /// # fn is_admin(_cx: &Cx) -> bool { true }
    /// fn post_policy(cx: &Cx, ability: Ability<'_, Post>) -> bool {
    ///     match ability {
    ///         _ if ability.is_header_action::<RecountTags>() => is_admin(cx),
    ///         Ability::ViewAny | Ability::View(_) => true,
    ///         _ => false,
    ///     }
    /// }
    /// ```
    pub fn is_header_action<A: HeaderAction>(self) -> bool {
        matches!(self, Ability::RunHeader { action } if action == A::NAME)
    }
}

impl<M> crate::policy::Abilities<M> {
    /// Who may run the header action `A` from the list's header:
    /// [`RunHeader`](Ability::RunHeader) with `A`'s [`NAME`](HeaderAction::NAME). A header
    /// action no `run_header` declares is refused.
    #[must_use]
    pub fn run_header<A: HeaderAction>(
        mut self,
        check: impl Fn(&Cx) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.set_header_action(A::NAME, Box::new(check));
        self
    }
}

/// The [`HeaderAction`]s a page or a resource declares, in button order.
///
/// ```rust
/// # use tablo_core::{HeaderAction, HeaderActions};
/// # use topcoat::{Result, context::Cx};
/// # struct ClearCache;
/// # impl HeaderAction for ClearCache {
/// #     type Input = ();
/// #     const NAME: &'static str = "clear-cache";
/// #     async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError> { Ok(tablo_core::Outcome::done()) }
/// # }
/// let actions = HeaderActions::new().add::<ClearCache>();
/// # let _ = actions;
/// ```
#[derive(Default)]
pub struct HeaderActions {
    entries: Vec<HeaderEntry>,
}

impl std::fmt::Debug for HeaderActions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(self.entries.iter().map(|e| e.name))
            .finish()
    }
}

impl HeaderActions {
    /// No action.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends the action `A`, failing to compile when `A::NAME` is not a single path segment.
    ///
    /// The check is a constant evaluated when `A` is instantiated, so `cargo check` does not
    /// report it; `cargo build` does. The doctest builds the program. Its `E0080` names the
    /// error, but rustdoc checks the code on nightly only.
    ///
    /// ```rust,compile_fail,E0080
    /// # use tablo_core::{HeaderAction, HeaderActions};
    /// # use topcoat::{Result, context::Cx};
    /// struct ClearCache;
    ///
    /// impl HeaderAction for ClearCache {
    ///     type Input = ();
    ///     const NAME: &'static str = "clear cache";
    ///
    ///     async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError> {
    ///         Ok(tablo_core::Outcome::done())
    ///     }
    /// }
    ///
    /// let _actions = HeaderActions::new().add::<ClearCache>();
    /// ```
    #[must_use]
    pub fn add<A: HeaderAction>(mut self) -> Self {
        const {
            assert!(
                crate::declaration::segment_fault(A::NAME).is_none(),
                "`HeaderAction::NAME` must be a single path segment"
            );
        }
        self.entries.push(HeaderEntry {
            name: A::NAME,
            label: A::label,
            can_run: A::can_run,
            input: InputSpec::of::<A::Input>(parse_header_input::<A>),
            run: run_header_erased::<A>,
            success: A::success,
            confirm: A::CONFIRM,
        });
        self
    }

    /// Whether no action is declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The declared actions, in order.
    pub(crate) fn entries(&self) -> &[HeaderEntry] {
        &self.entries
    }

    /// The action named `name`.
    pub(crate) fn find(&self, name: &str) -> Option<&HeaderEntry> {
        self.entries.iter().find(|e| e.name == name)
    }
}

/// An erased header action's body: it acts with the parsed input inside the transaction.
pub(crate) type RunHeader = for<'a> fn(
    &'a Cx,
    ErasedInput,
    &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<crate::Outcome, crate::WriteError>>;

/// A [`HeaderAction`] with its type erased.
#[derive(Clone, Copy)]
pub(crate) struct HeaderEntry {
    pub(crate) name: &'static str,
    pub(crate) label: fn(&Cx) -> String,
    pub(crate) can_run: fn(&Cx) -> bool,
    pub(crate) input: InputSpec,
    pub(crate) run: RunHeader,
    pub(crate) success: fn(&Cx) -> String,
    pub(crate) confirm: bool,
}

/// [`FormInput::parse`], then [`HeaderAction::validate_input`], behind a function pointer.
fn parse_header_input<A: HeaderAction>(cx: &Cx, values: &FormValues) -> InputResult {
    parse_validated(cx, values, A::validate_input)
}

/// [`HeaderAction::run`] behind a function pointer.
fn run_header_erased<'a, A: HeaderAction>(
    cx: &'a Cx,
    input: ErasedInput,
    ex: &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<crate::Outcome, crate::WriteError>> {
    let input = *input
        .downcast::<A::Input>()
        .expect("the pipeline parses the input `run` takes");
    Box::pin(A::run(cx, input, ex))
}
