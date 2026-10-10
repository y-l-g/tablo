//! App-defined field controls: the [`Control`] trait, what it renders from
//! ([`ControlInput`]), and the built-in [`Toggle`] written against it.

use tablo_ui::checkbox as ui_checkbox;
use topcoat::{context::Cx, view::*};

/// Renders a field's input from a [`ControlInput`], using a hidden field for a state that submits
/// nothing. A detail page shows the value through a [`Column`](crate::extend::Column) instead.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Theme { #[key] #[auto] id: uuid::Uuid, accent: String }
/// # use tablo_core::{
/// #     Field,
/// #     extend::{Control, ControlInput},
/// # };
/// # use topcoat::{context::Cx, view::*};
/// struct Color;
///
/// impl Control for Color {
///     fn render<'a>(&self, cx: &'a Cx, input: ControlInput) -> BoxView<'a> {
///         let attrs = input.attributes(cx);
///         view! { cx => <input type="color" (attrs)> }.boxed()
///     }
/// }
///
/// Field::custom(Theme::fields().accent(), Color);
/// ```
pub trait Control: Send + Sync {
    fn render<'a>(&self, cx: &'a Cx, input: ControlInput) -> BoxView<'a>;
}

/// What a [`Control`] renders for one form: the field's key, its DOM id, its current value, and
/// its validation state.
#[derive(Debug, Clone)]
pub struct ControlInput {
    name: String,
    id: String,
    value: Option<String>,
    required: bool,
    invalid: bool,
    described_by: Option<String>,
    disabled: bool,
}

impl ControlInput {
    pub(crate) fn new(
        name: &str,
        id: &str,
        value: Option<&str>,
        required: bool,
        invalid: bool,
        described_by: Option<String>,
    ) -> Self {
        Self {
            name: name.to_string(),
            id: id.to_string(),
            value: value.map(str::to_string),
            required,
            invalid,
            described_by,
            disabled: false,
        }
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// The control's DOM id, which its label names: the key, prefixed when its form shares the
    /// page with another, such as an action's input in a dialog.
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    pub fn required(&self) -> bool {
        self.required
    }

    pub fn invalid(&self) -> bool {
        self.invalid
    }

    pub fn described_by(&self) -> Option<&str> {
        self.described_by.as_deref()
    }

    /// Whether the control renders disabled: the server ignores what it posts.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    pub fn attributes(&self, cx: &Cx) -> Attributes {
        let required = self.required;
        attributes! {
            cx =>
            id=(self.id.clone())
            name=(self.name.clone())
            value=(self.value.clone())
            required=(required)
            disabled=(self.disabled)
            aria-required=(required.then_some("true"))
            aria-invalid=(if self.invalid { "true" } else { "false" })
            aria-describedby=(self.described_by.clone())
        }
    }
}

/// Renders a `bool` field as a checkbox preceded by a hidden `false`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Toggle;

impl Control for Toggle {
    fn render<'a>(&self, cx: &'a Cx, input: ControlInput) -> BoxView<'a> {
        let name = input.name().to_string();
        let id = input.id().to_string();
        let checked = input.value().is_some_and(|v| v.trim() == "true");
        let invalid = if input.invalid() { "true" } else { "false" };
        let described_by = input.described_by().map(str::to_string);
        let disabled = input.is_disabled();
        view! {
            cx =>
            <input type="hidden" name=(name.clone()) value="false" disabled=(disabled)>
            ui_checkbox(
                attrs: attributes! {
                    id=(id)
                    name=(name)
                    value="true"
                    checked=(checked)
                    disabled=(disabled)
                    aria-invalid=(invalid)
                    aria-describedby=(described_by)
                }
            )
        }
        .boxed()
    }
}
