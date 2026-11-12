//! [`HeaderActionDef`]: how a [`HeaderAction`](super::HeaderAction)'s button reads and who it
//! runs for.

use std::sync::Arc;

use topcoat::{context::Cx, icon::IconData};

use crate::{Color, Text};

/// A check on the request alone.
pub(crate) type RequestCheck = Arc<dyn Fn(&Cx) -> bool + Send + Sync>;

/// What a [`HeaderAction`](super::HeaderAction) declares besides its name, input and body: how
/// its button reads, whether it asks first, and who it runs for beyond the policy or the page's
/// access. Every setting has a default, so [`HeaderActionDef::new`] alone is a gray button
/// labelled after the action's `NAME`.
///
/// ```rust
/// # use tablo_core::{Color, HeaderActionDef};
/// let def = HeaderActionDef::new()
///     .icon(tablo_ui::icons::CHECK)
///     .color(Color::Warning)
///     .confirm();
/// # let _ = def;
/// ```
pub struct HeaderActionDef {
    pub(super) confirm: bool,
    pub(super) confirmation: Option<Text>,
    pub(super) icon: Option<IconData>,
    pub(super) color: Color,
    pub(super) label: Option<Text>,
    pub(super) can_run: Option<RequestCheck>,
    pub(super) success: Option<Text>,
}

impl Default for HeaderActionDef {
    fn default() -> Self {
        Self {
            confirm: false,
            confirmation: None,
            icon: None,
            color: Color::Gray,
            label: None,
            can_run: None,
            success: None,
        }
    }
}

impl std::fmt::Debug for HeaderActionDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeaderActionDef")
            .field("confirm", &self.confirm)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl HeaderActionDef {
    /// Every default: a gray button labelled after the action's `NAME`, running without
    /// confirmation for whoever the policy or the page's access admits.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks first, through a confirmation dialog with destructive wording.
    ///
    /// An action with input confirms in its input dialog, or on its input page, instead. An
    /// unconfirmed POST that would write answers 400.
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

    /// The icon before the button's label. Defaults to none.
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
    /// `NAME` in sentence case: `"recount-tags"` reads "Recount tags".
    #[must_use]
    pub fn label(mut self, label: impl Into<Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Whether the request may run the action, beyond the policy or the page's access: a
    /// feature flag, a role the policy does not model. Defaults to every request.
    #[must_use]
    pub fn can_run(mut self, check: impl Fn(&Cx) -> bool + Send + Sync + 'static) -> Self {
        self.can_run = Some(Arc::new(check));
        self
    }

    /// The success notification after a commit. Defaults to the label: `"Recount tags: done"`.
    #[must_use]
    pub fn success(mut self, message: impl Into<Text>) -> Self {
        self.success = Some(message.into());
        self
    }
}
