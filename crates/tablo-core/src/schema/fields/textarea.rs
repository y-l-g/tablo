use tablo_ui::textarea as ui_textarea;
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::{
        lenses::{FieldResolver, lens_field, lens_label},
        tree::Mode,
        validation::Rules,
    },
    FieldChrome, ValueKind, render_field, render_value,
};

/// Typed multi-line text field bound to a Toasty field lens.
///
/// `TextInput` renders `<input type="text">`, which is the wrong control for a
/// column holding prose — a post body, a description, a note. This is the same
/// field otherwise: one lens, the same lens-derived `required` default, the
/// same label and error contract, so the two are interchangeable in a `Schema`
/// and differ in the control (and in `TextInput`'s extra `email`/`unique`
/// modifiers, which have no textarea meaning).
///
/// A separate type rather than a `.multiline()` modifier on `TextInput`: the
/// control an author gets should be readable off the schema declaration, and
/// nothing in a Toasty `String` column says whether it holds prose.
#[derive(Debug, Clone)]
pub struct Textarea {
    name: String,
    label: String,
    required: bool,
    placeholder: Option<String>,
    rows: Option<u32>,
}

impl Textarea {
    /// Create a `Textarea` bound to the given field lens.
    ///
    /// Only `String` lenses compile, and `required` defaults from the field's
    /// nullability exactly as `TextInput::r#for` documents.
    ///
    /// There is deliberately no `.unique()`: the app-side unique check builds
    /// its probe from `TextInput::eq_filter`, so a uniqueness modifier here
    /// would be a no-op that reads like a guarantee (GH #115 tracks the real
    /// fix). Declare uniqueness on a `TextInput` instead.
    pub fn r#for<M>(path: toasty::stmt::Path<M, String>) -> Self
    where
        M: toasty::schema::Model,
    {
        let model = M::schema();
        let field = lens_field(path, &model);
        let label_str = lens_label(&field);
        Self {
            name: field.name.app_unwrap().to_string(),
            label: label_str,
            required: !field.nullable(),
            placeholder: None,
            rows: None,
        }
    }

    /// Create a `Textarea` bound to a lens inside an embedded struct or a
    /// `#[document]`, resolving through the request's app schema so
    /// the leaf arrives as its flattened storage column. Same contract as
    /// `TextInput::r#for_context`, including the not-required default.
    pub fn r#for_context<M>(cx: &Cx, path: toasty::stmt::Path<M, String>) -> Self
    where
        M: toasty::schema::Model,
    {
        let leaf = FieldResolver::from_cx(cx).resolve(path);
        Self {
            name: leaf.name,
            label: leaf.label,
            required: !leaf.nullable,
            placeholder: None,
            rows: None,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Opt out of the non-nullable default, as
    /// [`TextInput::optional`](crate::schema::TextInput::optional).
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = Some(p.into());
        self
    }

    /// Fix the control's visible height in lines. Unset defers to the
    /// primitive, which grows with its content from a two-line minimum.
    pub fn rows(mut self, rows: u32) -> Self {
        self.rows = Some(rows);
        self
    }

    pub fn label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }

    pub fn field_name(&self) -> &str {
        &self.name
    }

    /// Whether an empty submit fails validation (GH #88 semantics, as
    /// `TextInput::is_required`).
    pub fn is_required(&self) -> bool {
        self.required
    }

    pub fn validate(&self, value: &str) -> Vec<String> {
        Rules::new().validate(&self.label, self.required, value)
    }

    /// Static render: the same `field` family chrome as
    /// [`TextInput`](crate::schema::TextInput), with the stored value as the
    /// control's child (a `<textarea>` takes its initial value from content,
    /// not a `value` attribute).
    pub(crate) async fn render_with<'a>(
        &self,
        cx: &'a Cx,
        value: Option<&str>,
        errors: &[String],
        mode: Mode,
    ) -> Result<BoxView<'a>> {
        if mode == Mode::View {
            return render_value(cx, &self.label, value, ValueKind::Prose);
        }
        let name = self.name.clone();
        let required = self.required;
        let placeholder = self.placeholder.clone();
        let rows = self.rows;
        let value_owned = value.unwrap_or("").to_string();
        let chrome = FieldChrome::new(&name, errors, None);
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome.described_by();
        let control = view! {
            cx =>
            ui_textarea(
                attrs: attributes! {
                    id=(name.clone())
                    name=(name.clone())
                    placeholder=(placeholder.clone())
                    rows=(rows)
                    required=(required)
                    aria-required=(required.then_some("true"))
                    aria-invalid=(aria_invalid)
                    aria-describedby=(described_by)
                },
                (value_owned)
            )
        }
        .boxed();
        render_field(cx, &chrome, &self.label, required, attributes! {}, control)
    }
}

#[cfg(test)]
mod tests;
