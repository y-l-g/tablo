//! [`ActionDef`]: where an [`Action`](super::Action)'s button renders, how it reads, and which
//! records it runs on.

use std::sync::Arc;

use topcoat::{context::Cx, icon::IconData};

use super::Places;
use crate::{Color, Text, resource::Resource};

/// A closure over the request and one record.
pub(crate) type RecordFn<M, T> = Arc<dyn Fn(&Cx, &M) -> T + Send + Sync>;

/// A message over a record count.
pub(crate) type CountFn = Arc<dyn Fn(usize) -> Text + Send + Sync>;

/// What an [`Action`](super::Action) declares besides its name, input and body: where its button
/// renders, how it reads on each record, and which records it runs on. Every setting has a
/// default, so [`ActionDef::new`] alone is a button on each row, page header and the bulk bar,
/// labelled after the action's `NAME`, that runs on every record the policy allows.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::PanelModel)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     status: String,
/// # }
/// # use tablo_core::{ActionDef, Color, NoForm, Places, Resource};
/// # struct PostResource;
/// # impl Resource for PostResource {
/// #     type Model = Post;
/// #     type Form = NoForm<Post>;
/// # }
/// let def = ActionDef::<PostResource>::new()
///     .places(Places::ROW.with(Places::DETAIL))
///     .icon(tablo_ui::icons::CHECK)
///     .color(Color::Success)
///     .can_run(|_cx, post: &Post| post.status != "published")
///     .disabled_reason(|_cx, _post| Some("Already published".into()));
/// # let _ = def;
/// ```
pub struct ActionDef<R: Resource> {
    pub(super) places: Places,
    pub(super) confirm: bool,
    pub(super) confirmation: Option<Text>,
    pub(super) icon: Option<IconData>,
    pub(super) color: Color,
    pub(super) label: Option<Text>,
    pub(super) label_for: Option<RecordFn<R::Model, Text>>,
    pub(super) icon_for: Option<RecordFn<R::Model, Option<IconData>>>,
    pub(super) tooltip: Option<Text>,
    pub(super) can_run: Option<RecordFn<R::Model, bool>>,
    pub(super) disabled_reason: Option<RecordFn<R::Model, Option<Text>>>,
    pub(super) success: Option<CountFn>,
}

impl<R: Resource> Default for ActionDef<R> {
    fn default() -> Self {
        Self {
            places: Places::ALL,
            confirm: false,
            confirmation: None,
            icon: None,
            color: Color::Gray,
            label: None,
            label_for: None,
            icon_for: None,
            tooltip: None,
            can_run: None,
            disabled_reason: None,
            success: None,
        }
    }
}

impl<R: Resource> std::fmt::Debug for ActionDef<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActionDef")
            .field("places", &self.places)
            .field("confirm", &self.confirm)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl<R: Resource> ActionDef<R> {
    /// Every default: a gray button on each row, page header and the bulk bar, labelled after the
    /// action's `NAME`, running without confirmation on every record the policy allows.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Where the button renders. Defaults to [`Places::ALL`]: each row, the detail and edit
    /// pages' headers, and the bulk bar.
    ///
    /// It decides where the button shows, not who may run it: the record route serves an action
    /// placed on any of [`Places::RECORD`], and the bulk route one placed on [`Places::BULK`];
    /// any other POST answers 404. A place no page renders shows the button nowhere:
    /// [`Places::EDIT`] on a resource without a form, or [`Places::DETAIL`] on one that turns its
    /// view page off ([`ResourcePage::View`](crate::ResourcePage::View)).
    #[must_use]
    pub fn places(mut self, places: Places) -> Self {
        self.places = places;
        self
    }

    /// Asks first, through a confirmation dialog sharing the delete dialog's mechanism and
    /// destructive wording.
    ///
    /// An action with input confirms in its input dialog, or on its input page, instead, which
    /// say the action cannot be undone and whose submit renders destructive. An unconfirmed POST
    /// that would write answers 400. Both dialogs need JavaScript: without it their buttons do
    /// nothing.
    #[must_use]
    pub fn confirm(mut self) -> Self {
        self.confirm = true;
        self
    }

    /// Asks first, as [`confirm`](Self::confirm) does, with `question` in place of the message
    /// `tablo-confirm-run`, "Run this action?". An action with input confirms in its input
    /// dialog instead, which its label titles.
    #[must_use]
    pub fn confirmation(mut self, question: impl Into<Text>) -> Self {
        self.confirm = true;
        self.confirmation = Some(question.into());
        self
    }

    /// The icon before the button's label, in every place. Defaults to none.
    #[must_use]
    pub fn icon(mut self, icon: IconData) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The colour the button is tinted in, as Delete's is in [`Color::Danger`]. Defaults to
    /// [`Color::Gray`], the untinted button.
    ///
    /// It changes how the button looks, not what it asks: [`confirm`](Self::confirm) asks first.
    #[must_use]
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// The button text. Defaults to the message `action-{NAME}` of the panel's catalogs, else
    /// `NAME` in sentence case: `"publish"` reads "Publish", and `"send-invite"` reads "Send
    /// invite".
    ///
    /// The bulk bar, the input dialogs and the messages read it; a record's button reads
    /// [`label_for`](Self::label_for) when one is declared.
    #[must_use]
    pub fn label(mut self, label: impl Into<Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The text of the button on one record: its row's, or its record page's. Defaults to the
    /// [`label`](Self::label).
    ///
    /// Declare it when the record's state changes what the action reads: "Deactivate" on an
    /// active employee, "Activate" on one that is not.
    #[must_use]
    pub fn label_for(
        mut self,
        label: impl Fn(&Cx, &R::Model) -> Text + Send + Sync + 'static,
    ) -> Self {
        self.label_for = Some(Arc::new(label));
        self
    }

    /// The icon of the button on one record, in its row and on its record page. Defaults to the
    /// [`icon`](Self::icon).
    #[must_use]
    pub fn icon_for(
        mut self,
        icon: impl Fn(&Cx, &R::Model) -> Option<IconData> + Send + Sync + 'static,
    ) -> Self {
        self.icon_for = Some(Arc::new(icon));
        self
    }

    /// The hint a row's button shows on hover, for an action whose label says too little.
    /// Defaults to none.
    ///
    /// A button refused on its record shows its [`disabled_reason`](Self::disabled_reason)
    /// instead.
    #[must_use]
    pub fn tooltip(mut self, tooltip: impl Into<Text>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Whether the action itself runs on a record, wherever it is mounted: a published post
    /// refuses "publish". Defaults to every record.
    ///
    /// It is one half of [`Ability::Run`](crate::Ability::Run), which the resource allows only
    /// when both this and its policy do: [`can`](crate::can) answers the two together. Who may
    /// run the action belongs in the policy, which each panel mounts its own; a rule written here
    /// holds on every panel and cannot be lifted by one. Asking `can` for this action's own `Run`
    /// from here would ask this closure again.
    #[must_use]
    pub fn can_run(
        mut self,
        check: impl Fn(&Cx, &R::Model) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.can_run = Some(Arc::new(check));
        self
    }

    /// Why the button shows disabled on a record [`can_run`](Self::can_run) refuses, rather than
    /// hidden: "Already published" for a published post. Defaults to none, which hides it.
    ///
    /// It is asked only of a record the policy lets the user view and run the action on, so the
    /// reason speaks to the record's state and never tells a user what the policy refuses them.
    /// The route refuses a disabled record as it refuses one `can_run` hides.
    #[must_use]
    pub fn disabled_reason(
        mut self,
        reason: impl Fn(&Cx, &R::Model) -> Option<Text> + Send + Sync + 'static,
    ) -> Self {
        self.disabled_reason = Some(Arc::new(reason));
        self
    }

    /// The success notification after a commit that ran on `count` records. Defaults to the
    /// label and the record count: `"Publish: 3 records"`. A bulk run the action refused on some
    /// records appends their count out of the selection: `"Publish: 3 records (2 of 5
    /// skipped)"`.
    #[must_use]
    pub fn success(mut self, message: impl Fn(usize) -> Text + Send + Sync + 'static) -> Self {
        self.success = Some(Arc::new(message));
        self
    }
}
