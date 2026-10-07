//! App-defined field controls: the [`Control`] trait, what it renders from
//! ([`ControlInput`]), and the built-in [`Toggle`] written against it.

use tablo_ui::checkbox as ui_checkbox;
use topcoat::{context::Cx, view::*};

/// Renders a field's input from a [`ControlInput`], using a hidden field for a state that submits
/// nothing. A detail page shows the value through a [`Column`](crate::Column) instead.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Theme { #[key] #[auto] id: uuid::Uuid, accent: String }
/// # use tablo_core::{Control, ControlInput, Field};
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

/// What a [`Control`] renders for one form: the field's key, its current
/// value, and its validation state.
#[derive(Debug, Clone)]
pub struct ControlInput {
    name: String,
    value: Option<String>,
    required: bool,
    invalid: bool,
    described_by: Option<String>,
}

impl ControlInput {
    pub(crate) fn new(
        name: &str,
        value: Option<&str>,
        required: bool,
        invalid: bool,
        described_by: Option<String>,
    ) -> Self {
        Self {
            name: name.to_string(),
            value: value.map(str::to_string),
            required,
            invalid,
            described_by,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
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

    pub fn attributes(&self, cx: &Cx) -> Attributes {
        let required = self.required;
        attributes! {
            cx =>
            id=(self.name.clone())
            name=(self.name.clone())
            value=(self.value.clone())
            required=(required)
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
        let checked = input.value().is_some_and(|v| v.trim() == "true");
        let invalid = if input.invalid() { "true" } else { "false" };
        let described_by = input.described_by().map(str::to_string);
        view! {
            cx =>
            <input type="hidden" name=(name.clone()) value="false">
            ui_checkbox(
                attrs: attributes! {
                    id=(name.clone())
                    name=(name)
                    value="true"
                    checked=(checked)
                    aria-invalid=(invalid)
                    aria-describedby=(described_by)
                }
            )
        }
        .boxed()
    }
}
