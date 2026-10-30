//! Custom actions: the [`Action`] trait, and the [`Actions`] list a
//! [`ResourceDef`](super::ResourceDef) declares them in.

use std::{any::Any, future::Future};

use derive_where::derive_where;
use topcoat::{Result, context::Cx, icon::IconData};

mod abilities;
mod builtin;
mod keys;
mod places;

pub use keys::filter_keys;
pub use places::Places;

use super::{Mounted, Resource, WriteError};
use crate::{
    BoxFuture,
    form::{FieldError, FieldErrors, FormInput, FormValues},
    naming::{derived_id, sentence_case},
    policy::Ability,
    schema::Schema,
};

/// The keys an action's POST carries besides its input, which no input field may post.
pub(crate) const RESERVED_KEYS: [&str; 4] = [crate::csrf::FIELD_NAME, "confirm", "ids", "matching"];

/// A mutation beyond create, update and delete: "publish", "archive",
/// "resend the invite".
///
/// An action runs on one record, from a button in its table row or in the header of its detail or
/// edit page, or on the selection, from the bulk bar: [`PLACES`](Self::PLACES) says which.
/// [`ResourceDef::action`](super::ResourceDef::action) declares it:
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
///     async fn run(
///         _cx: &Cx,
///         posts: &[Post],
///         _: (),
///         ex: &mut dyn Executor,
///     ) -> Result<tablo_core::Outcome, tablo_core::WriteError> {
///         Post::filter(tablo_core::filter_keys(posts))
///             .update()
///             .status("published".to_string())
///             .exec(ex)
///             .await?;
///         Ok(tablo_core::Outcome::done())
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
/// - the hooks, with [`Change::Action`](super::Change::Action): [`Resource::before_commit`] in the
///   transaction after `run`, then [`Resource::after_commit`] once it commits; and the success
///   notification.
///
/// An action that asks for an [`Input`](Self::Input) asks for it first: its button opens the
/// input's form in a dialog over the page, whose submit runs the action with the parsed value. The
/// submit parses the input, asks [`validate_input`](Self::validate_input) and checks its choices
/// before the transaction opens, then re-checks a relationship choice inside it. A refused value
/// renders the input as a page with its errors and writes nothing; a key the input does not
/// declare answers 400. The page is titled with the label and the record's title, or the record
/// count for a selection, and its Cancel returns to the page the button was on. Cancel and Escape
/// close the dialog, and opening it again starts from a blank form. A searchable relationship
/// choice searches `{list}/-/actions/{NAME}/options`, behind the checks the POST makes before it
/// loads a record. The dialog needs JavaScript.
///
/// A run from a detail or edit page lands back on that page, its query kept; one from a row or
/// the bulk bar lands on the list. An action that takes the record out of the resource's
/// [`query`](super::ResourceDef::query) or its `View`, such as an archive the list hides, lands on
/// that page's 404: place it on rows and the bulk bar instead.
///
/// The bulk bar of a resource's list whose rows span pages offers Select all matching: the
/// selection then takes every record the list's search and filters select, on any page, rather
/// than the rows checked. The POST carries the list's query, and the route re-runs it inside the
/// transaction, so it takes the records that match when it runs. A record the user may not view
/// is skipped as a refused one is, and more matches than
/// [`Limits::bulk_records`](crate::Limits::bulk_records) write nothing. A filter the list would
/// ignore answers 400.
///
/// No place renders a button for an action the policy refuses `RunAny`, and no row or page renders
/// one for a record that fails `View` or `Run`. A record `can_run` refuses shows its button
/// disabled, saying why, when [`disabled_reason`](Self::disabled_reason) gives a reason, and none
/// otherwise. A row that no bulk action and no delete allows renders no checkbox.
pub trait Action<R: Resource>: 'static {
    /// What the action asks for before it runs: `()` for nothing, or an
    /// [`FormInput`](derive@crate::FormInput) struct, such as a rejection's reason.
    type Input: FormInput;

    /// The action's URL segment, distinct among the resource's actions.
    ///
    /// [`ResourceDef::action`](super::ResourceDef::action) refuses to compile a name that is not
    /// a single path segment: empty, `.` or `..`, or holding whitespace, a control character, a
    /// quote, a backslash or one of `/ ? # % & = { } ( )`.
    /// [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel) refuses a name that
    /// another action of the resource shares.
    ///
    /// ```rust,compile_fail,E0080
    /// # use tablo_core::{Action, NoForm, Resource, ResourceDef};
    /// # use topcoat::{Result, context::Cx};
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
    /// #   async fn run(_: &Cx, _: &[Post], _: (), _: &mut dyn toasty::Executor) -> Result<tablo_core::Outcome, tablo_core::WriteError> { Ok(tablo_core::Outcome::done()) }
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
    /// [`Places::BULK`]; any other POST answers 404. A place no page renders shows the button
    /// nowhere: [`Places::EDIT`] on a resource without a form, or [`Places::DETAIL`] on one that
    /// turns its view page off ([`ResourcePage::View`](crate::ResourcePage::View)).
    const PLACES: Places = Places::ALL;

    /// Whether the action asks first through a confirmation dialog sharing the
    /// delete dialog's mechanism and destructive wording. Defaults to `false`.
    ///
    /// An action with input confirms in its input dialog, or on its input page, instead, which
    /// say the action cannot be undone and whose submit renders destructive. An unconfirmed POST
    /// that would write answers 400. Both dialogs need JavaScript: without it their buttons do
    /// nothing.
    const CONFIRM: bool = false;

    /// The icon before the button's label, in every place. Defaults to none.
    ///
    /// ```rust
    /// # use topcoat::icon::IconData;
    /// const ICON: Option<IconData> = Some(tablo_ui::icons::CHECK);
    /// ```
    const ICON: Option<IconData> = None;

    /// Whether the button is tinted as a destructive one, as Delete's is. Defaults to `false`.
    ///
    /// It changes how the button looks, not what it asks: [`CONFIRM`](Self::CONFIRM) asks first.
    const DESTRUCTIVE: bool = false;

    /// The question the confirmation dialog asks when [`CONFIRM`](Self::CONFIRM) is set. Defaults
    /// to the message `tablo-confirm-run`, "Run this action?". An action with input confirms in
    /// its input dialog instead, which its label titles.
    fn confirmation() -> crate::Text {
        crate::text!("tablo-confirm-run")
    }

    /// The button text. Defaults to the message `action-{NAME}` of the panel's catalogs, else
    /// [`NAME`](Self::NAME) in sentence case: `"publish"` reads "Publish", and `"send-invite"`
    /// reads "Send invite".
    fn label() -> crate::Text {
        crate::Text::derived(derived_id("action", Self::NAME), sentence_case(Self::NAME))
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

    /// Why the button shows disabled on a record [`can_run`](Self::can_run) refuses, rather than
    /// hidden: "Already published" for a published post. Defaults to none, which hides it.
    ///
    /// It is asked only of a record the policy lets the user view and run the action on, so the
    /// reason speaks to the record's state and never tells a user what the policy refuses them.
    /// The route refuses a disabled record as it refuses one `can_run` hides.
    fn disabled_reason(_cx: &Cx, _record: &R::Model) -> Option<crate::Text> {
        None
    }

    /// Refuse an input [`run`](Self::run) should not receive, such as a reason too short to
    /// act on: each error names a field of the input's [`Field`](FormInput::Field) enum and
    /// renders under its control. Defaults to none.
    ///
    /// ```rust
    /// # use tablo_core::{FieldErrors, FormInput};
    /// # #[derive(tablo_core::FormInput)]
    /// # struct Closing { reason: String }
    /// # fn validate_input(_cx: &topcoat::context::Cx, closing: &Closing) -> FieldErrors<ClosingField> {
    /// let mut errors = FieldErrors::new();
    /// if closing.reason.trim().len() < 10 {
    ///     errors.add(ClosingField::Reason, "Say why in ten characters or more");
    /// }
    /// # errors
    /// # }
    /// ```
    ///
    /// It runs after the input parses and before the transaction opens, so it sees no record; a
    /// rule that reads the database refuses from [`run`](Self::run).
    fn validate_input(
        _cx: &Cx,
        _input: &Self::Input,
    ) -> FieldErrors<<Self::Input as FormInput>::Field> {
        FieldErrors::new()
    }

    /// Perform the action on `records`, the records of the row or selection that
    /// passed [`Ability::Run`] and [`can_run`](Self::can_run), with the parsed `input`, through
    /// the framework's transaction `ex`, and say what the response does once it commits:
    /// [`Outcome::done`](crate::Outcome::done) for the [`success`](Self::success) notification,
    /// landing back where the action ran from. [`filter_keys`](crate::filter_keys) writes one
    /// change to every record in one statement.
    ///
    /// A rule that reads the database refuses the input with [`WriteError::Refused`], keyed by
    /// its form keys: the transaction rolls back and the input page renders with the errors. An
    /// action that asks for nothing has no control to render them under, so its refusal is a
    /// declaration error: it declines with [`WriteError::declined`] instead, which rolls back and
    /// flashes the message.
    fn run(
        cx: &Cx,
        records: &[R::Model],
        input: Self::Input,
        ex: &mut dyn toasty::Executor,
    ) -> impl Future<Output = Result<crate::Outcome, WriteError>> + Send;

    /// The success notification after a commit. Defaults to the label and
    /// the record count: `"Publish: 3 records"`. A bulk run the action refused
    /// on some records appends their count out of the selection:
    /// `"Publish: 3 records (2 of 5 skipped)"`.
    fn success(count: usize) -> crate::Text {
        crate::text!(
            "tablo-action-on-records",
            action = Self::label(),
            count = count
        )
    }
}

/// An erased action's body: it acts on the records with the parsed input inside the transaction.
pub(crate) type RunAction<R> = for<'a> fn(
    &'a Cx,
    &'a [<R as Resource>::Model],
    ErasedInput,
    &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<crate::Outcome, WriteError>>;

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
    pub(crate) parse: fn(&Cx, &FormValues) -> InputResult,
}

impl InputSpec {
    /// The input of type `I`, parsed and validated by `parse`.
    pub(crate) fn of<I: FormInput>(parse: fn(&Cx, &FormValues) -> InputResult) -> Self {
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
            disabled: disabled_erased::<R, A>,
            input: InputSpec::of::<A::Input>(parse_input_erased::<R, A>),
            run: run_erased::<R, A>,
            success: A::success,
            change: super::acted::<R, A>,
            failure: "tablo-failed-run",
            confirm: A::CONFIRM,
            trashed: super::Trashed::Without,
            removes: false,
            prompt: |_| A::confirmation(),
            icon: || A::ICON,
            destructive: A::DESTRUCTIVE,
        });
        self
    }

    /// Appends the built-in restore and force delete a soft-deleting resource offers on its
    /// trashed records.
    pub(crate) fn add_trash_actions(&mut self) {
        self.entries.push(ActionEntry::restore());
        self.entries.push(ActionEntry::force_delete());
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
#[derive_where(Clone, Copy)]
pub(crate) struct ActionEntry<R: Resource> {
    pub(crate) name: &'static str,
    pub(crate) label: fn() -> crate::Text,
    pub(crate) places: Places,
    /// The resource-wide ability checked before the body is read.
    pub(crate) resource_wide: Ability<'static, R::Model>,
    /// Whether the mutation may write `record`, which already passed `View`.
    pub(crate) can_run: fn(&Mounted<R>, &Cx, &R::Model) -> bool,
    /// Why its button shows disabled on `record`, which already passed `View`, when the policy
    /// allows it and the record's state refuses it.
    pub(crate) disabled: fn(&Mounted<R>, &Cx, &R::Model) -> Option<crate::Text>,
    pub(crate) input: InputSpec,
    pub(crate) run: RunAction<R>,
    pub(crate) success: fn(usize) -> crate::Text,
    /// What a run on the records changed, for the hooks.
    pub(crate) change: fn(Vec<R::Model>) -> super::Change<R::Model>,
    /// The message the failure toast's title reads.
    pub(crate) failure: &'static str,
    pub(crate) confirm: bool,
    /// The records the mutation takes: the live ones, or the trashed ones.
    pub(crate) trashed: super::Trashed,
    /// Whether the mutation removes its records, so its button lands on the list rather than on
    /// the page of a record that no longer exists.
    pub(crate) removes: bool,
    /// The question the dialog confirming it asks, of one record or, when `bulk`, of a selection.
    pub(crate) prompt: fn(bool) -> crate::Text,
    /// The icon before its button's label.
    pub(crate) icon: fn() -> Option<IconData>,
    /// Whether its button is tinted as a destructive one.
    pub(crate) destructive: bool,
}

impl<R: Resource> ActionEntry<R> {
    /// Whether the mutation may write `record`, which already passed `View`: the record is live
    /// or trashed as the mutation takes it, and the mutation's own check allows it.
    pub(crate) fn allows(&self, resource: &Mounted<R>, cx: &Cx, record: &R::Model) -> bool {
        self.trashed.admits(resource.is_trashed(record)) && (self.can_run)(resource, cx, record)
    }

    /// Why the mutation's button shows disabled on `record`, which already passed `View`: the
    /// record is live or trashed as the mutation takes it, and the reason its own check refuses
    /// it, if it gives one. `None` for a record it allows.
    pub(crate) fn disabled_on(
        &self,
        resource: &Mounted<R>,
        cx: &Cx,
        record: &R::Model,
    ) -> Option<crate::Text> {
        if !self.trashed.admits(resource.is_trashed(record)) {
            return None;
        }
        (self.disabled)(resource, cx, record)
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

/// [`Action::disabled_reason`] for a record the mounted policy allows [`Ability::Run`] on and
/// [`Action::can_run`] refuses, behind a function pointer.
fn disabled_erased<R: Resource, A: Action<R>>(
    resource: &Mounted<R>,
    cx: &Cx,
    record: &R::Model,
) -> Option<crate::Text> {
    let run = Ability::Run {
        action: A::NAME,
        record,
    };
    if !resource.can(cx, run) || A::can_run(cx, record) {
        return None;
    }
    A::disabled_reason(cx, record)
}

/// [`FormInput::parse`], then [`Action::validate_input`], behind a function pointer.
fn parse_input_erased<R: Resource, A: Action<R>>(cx: &Cx, values: &FormValues) -> InputResult {
    parse_validated(cx, values, A::validate_input)
}

/// `I`'s [`FormInput::parse`], then `validate`, its errors keyed by the form keys they name.
pub(crate) fn parse_validated<I: FormInput>(
    cx: &Cx,
    values: &FormValues,
    validate: fn(&Cx, &I) -> FieldErrors<I::Field>,
) -> InputResult {
    let input = I::parse(cx, values)?;
    let refused: Vec<FieldError> = validate(cx, &input)
        .into_iter()
        .map(|error| FieldError {
            key: I::key(error.key).to_string(),
            kind: error.kind,
        })
        .collect();
    if !refused.is_empty() {
        return Err(refused);
    }
    Ok(Box::new(input))
}

/// A delete's input: nothing.
fn parse_nothing(_cx: &Cx, _values: &FormValues) -> InputResult {
    Ok(Box::new(()))
}

/// [`Action::run`] behind a function pointer.
fn run_erased<'a, R: Resource, A: Action<R>>(
    cx: &'a Cx,
    records: &'a [R::Model],
    input: ErasedInput,
    ex: &'a mut dyn toasty::Executor,
) -> BoxFuture<'a, Result<crate::Outcome, WriteError>> {
    let input = *input
        .downcast::<A::Input>()
        .expect("the pipeline parses the input `run` takes");
    Box::pin(A::run(cx, records, input, ex))
}
