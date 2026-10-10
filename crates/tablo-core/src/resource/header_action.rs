//! Actions on no record: the [`HeaderAction`] trait, and the [`HeaderActions`] a
//! [`Page`](crate::Page) or a [`ResourceDef`](super::ResourceDef) declares them in.

use std::{collections::HashMap, future::Future};

use topcoat::{Result, context::Cx};

use super::{ActionFuture, ActionInput, ErasedInput, InputResult, InputSpec, parse_validated};
use crate::{form::FieldErrors, naming::sentence_case, policy::Ability};

/// A mutation that acts on no record, run from a button in a page's header: "import posts",
/// "clear the cache", "recompute the totals".
///
/// A resource declares one with
/// [`ResourceDef::header_action`](crate::ResourceDef::header_action), and its list page renders
/// the button; a [`Page`](crate::Page) declares one in
/// [`Page::header_actions`](crate::Page::header_actions), and renders the buttons with
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
///     async fn run(_cx: &Cx, _: (), _ex: &mut dyn Executor) -> Result<()> {
///         // Recount through `ex`, the framework's transaction.
///         Ok(())
///     }
/// }
/// ```
///
/// The framework owns everything around [`run`](Self::run), as it does for a record's
/// [`Action`](crate::Action):
///
/// - the route, `{url}/-/actions/{NAME}` under the list or page URL, and its CSRF check;
/// - who may run it: on a resource, the policy's [`Ability::ViewAny`] and [`Ability::RunHeader`]
///   with the action's `NAME`; on a page, [`Page::can_access`](crate::Page::can_access); on either,
///   then [`can_run`](Self::can_run). A refusal answers 403 and renders no button;
/// - the transaction: `run` writes through its executor, and an error rolls everything back;
/// - on a resource, [`Resource::after_commit`](crate::Resource::after_commit) with
///   [`Mutation::Action`](crate::Mutation::Action) and no records once the transaction commits;
/// - the success notification, and the redirect back to the list or page.
///
/// An action that asks for an [`Input`](Self::Input) renders it as a form page first, as a
/// record's action does, and runs on its submit.
pub trait HeaderAction: 'static {
    /// What the action asks for before it runs: `()` for nothing, or an
    /// [`ActionInput`](derive@crate::ActionInput) struct.
    type Input: ActionInput;

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

    /// The button text. Defaults to [`NAME`](Self::NAME) in sentence case: `"recount-tags"`
    /// reads "Recount tags".
    fn label(_cx: &Cx) -> String {
        sentence_case(Self::NAME)
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

    /// Perform the action with the parsed `input`, through the framework's transaction `ex`.
    fn run(
        cx: &Cx,
        input: Self::Input,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = Result<()>> + Send;

    /// The success notification after a commit. Defaults to the label: `"Recount tags: done"`.
    fn success(cx: &Cx) -> String {
        format!("{}: done", Self::label(cx))
    }
}

impl<M> Ability<'_, M> {
    /// Whether the ability is [`RunHeader`](Ability::RunHeader) for the header action `A`, which
    /// a resource's policy answers to let the request run it.
    ///
    /// ```rust
    /// # use tablo_core::{Ability, HeaderAction};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid }
    /// # struct RecountTags;
    /// # impl HeaderAction for RecountTags {
    /// #     type Input = ();
    /// #     const NAME: &'static str = "recount-tags";
    /// #     async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> Result<()> { Ok(()) }
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

/// The [`HeaderAction`]s a page or a resource declares, in button order.
///
/// ```rust
/// # use tablo_core::{HeaderAction, HeaderActions};
/// # use topcoat::{Result, context::Cx};
/// # struct ClearCache;
/// # impl HeaderAction for ClearCache {
/// #     type Input = ();
/// #     const NAME: &'static str = "clear-cache";
/// #     async fn run(_: &Cx, _: (), _: &mut dyn toasty::Executor) -> Result<()> { Ok(()) }
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

/// A [`HeaderAction`] with its type erased.
#[derive(Clone, Copy)]
pub(crate) struct HeaderEntry {
    pub(crate) name: &'static str,
    pub(crate) label: fn(&Cx) -> String,
    pub(crate) can_run: fn(&Cx) -> bool,
    pub(crate) input: InputSpec,
    pub(crate) run:
        for<'a> fn(&'a Cx, ErasedInput, &'a mut dyn toasty::Executor) -> ActionFuture<'a>,
    pub(crate) success: fn(&Cx) -> String,
    pub(crate) confirm: bool,
}

/// [`ActionInput::parse`], then [`HeaderAction::validate_input`], behind a function pointer.
fn parse_header_input<A: HeaderAction>(cx: &Cx, values: &HashMap<String, String>) -> InputResult {
    parse_validated(cx, values, A::validate_input)
}

/// [`HeaderAction::run`] behind a function pointer.
fn run_header_erased<'a, A: HeaderAction>(
    cx: &'a Cx,
    input: ErasedInput,
    ex: &'a mut dyn toasty::Executor,
) -> ActionFuture<'a> {
    let input = *input
        .downcast::<A::Input>()
        .expect("the pipeline parses the input `run` takes");
    Box::pin(A::run(cx, input, ex))
}
