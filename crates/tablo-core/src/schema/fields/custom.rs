//! App-defined field controls: the [`Control`] trait, what it renders from
//! ([`ControlInput`]), and the built-in [`Toggle`] written against it.

use topcoat::{context::Cx, view::*};

/// Renders a field's input from a [`ControlInput`] and its stored value for display, using a hidden
/// field for a state that submits nothing.
///
/// ```ignore
/// struct Color;
///
/// impl Control for Color {
///     fn render<'a>(&self, cx: &'a Cx, input: ControlInput) -> BoxView<'a> {
///         let attrs = input.attributes(cx);
///         view! { cx => <input type="color" (attrs)> }.boxed()
///     }
/// }
///
/// Field::custom(Theme::fields().accent(), Color)
/// ```
pub trait Control: Send + Sync {
    fn render<'a>(&self, cx: &'a Cx, input: ControlInput) -> BoxView<'a>;

    /// Shows the stored value, as text by default.
    fn display<'a>(&self, cx: &'a Cx, value: &str) -> BoxView<'a> {
        let text = value.to_string();
        view! {
            cx =>
            <div class="text-sm text-foreground wrap-anywhere whitespace-pre-wrap">
                (text)
            </div>
        }
        .boxed()
    }
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

/// Renders a `bool` field as a checkbox preceded by a hidden `false`, displaying as `"Yes"`/`"No"`.
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
            <input
                type="checkbox"
                id=(name.clone())
                name=(name)
                value="true"
                checked=(checked)
                aria-invalid=(invalid)
                aria-describedby=(described_by)
                class="size-4 accent-primary"
            >
        }
        .boxed()
    }

    fn display<'a>(&self, cx: &'a Cx, value: &str) -> BoxView<'a> {
        let text = if value.trim() == "true" { "Yes" } else { "No" };
        view! { cx => <div class="text-sm text-foreground">(text)</div> }.boxed()
    }
}
