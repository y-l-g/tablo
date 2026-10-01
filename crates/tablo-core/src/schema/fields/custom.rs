//! App-defined field controls: the [`Control`] trait, what it renders from
//! ([`ControlInput`]), and the built-in [`Toggle`] written against it.

use topcoat::{context::Cx, view::*};

/// A form control a [`Field`](super::Field) renders through
/// [`Field::custom`](super::Field::custom).
///
/// The field keeps what every field shares: its key, its label, the required
/// rule, validation, the error slot, and the chrome around the control. The
/// control renders the input itself, from a [`ControlInput`], and, on the
/// detail page, the stored value.
///
/// The submission is read like any other field's: the form field named
/// [`ControlInput::name`] is the value, last one wins. A control that submits
/// nothing in some state (a checkbox left unchecked) renders a hidden field of
/// the same name before it, carrying the value that state means, as
/// [`Toggle`] does.
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
    /// The input, named and identified by [`ControlInput::name`].
    fn render<'a>(&self, cx: &'a Cx, input: ControlInput) -> BoxView<'a>;

    /// The stored value as the detail page shows it. Defaults to the value as
    /// text.
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

    /// The field's key: the control's `name` and `id`, which the field's
    /// label points at.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The value to show: the record's stored value on an edit, the
    /// submission on a re-rendered form, `None` on an empty create.
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    /// Whether an empty submission fails validation.
    pub fn required(&self) -> bool {
        self.required
    }

    /// Whether the field carries an error.
    pub fn invalid(&self) -> bool {
        self.invalid
    }

    /// The attributes a single input carries: `id`, `name`, `value`,
    /// `required`, `aria-required`, `aria-invalid`, and `aria-describedby`
    /// pointing at the error while there is one.
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

/// A checkbox over a `bool` field, built on [`Control`] alone:
/// [`Field::toggle`](super::Field::toggle) declares one.
///
/// A hidden `false` precedes the checkbox under the same name, so an
/// unchecked box submits `false` rather than nothing; a checked one submits
/// `true` after it, and the last value wins. The detail page reads
/// `"Yes"`/`"No"`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Toggle;

impl Control for Toggle {
    fn render<'a>(&self, cx: &'a Cx, input: ControlInput) -> BoxView<'a> {
        let name = input.name().to_string();
        let checked = input.value().is_some_and(|v| v.trim() == "true");
        let invalid = if input.invalid() { "true" } else { "false" };
        let described_by = input.described_by.clone();
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
