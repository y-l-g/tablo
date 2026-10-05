//! One [`Field`] type whose control is text, choice, file, or an app's own [`Control`], built from
//! a path binding a column or an embedded leaf.

mod builders;
mod choice;
mod custom;
mod file;
mod text;

use std::sync::Arc;

pub use builders::{ChoiceField, CustomField, FileField, IntoOptions, TextField};
pub(crate) use choice::{ChoiceControl, option_view};
pub use custom::{Control, ControlInput, Toggle};
use tablo_ui::{
    field as ui_field, field_content as ui_field_content, field_error as ui_field_error,
    field_label as ui_field_label, field_title as ui_field_title,
};
pub(crate) use text::TextControl;
use toasty::stmt::Path;
use topcoat::{Result, context::Cx, view::*};

use super::{lenses::ResolvedLens, tree::Mode, validation::Rules};
use crate::{
    form::{FieldError, FormScalar},
    naming::capitalize,
};

/// One form field binding a lens to the control editing it, offering only that control's modifiers
/// so a modifier on the wrong control does not compile.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     name: String,
/// #     email: String,
/// #     age: i64,
/// # }
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     body: String,
/// #     status: String,
/// #     author_id: uuid::Uuid,
/// #     featured: bool,
/// # }
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Doc { #[key] #[auto] id: uuid::Uuid, path: String }
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct Author { #[key] #[auto] id: uuid::Uuid, name: String }
/// # #[derive(Debug, Clone, Copy, PartialEq, Eq, tablo_core::Options)]
/// # enum Status { Draft, Published }
/// # struct AuthorResource;
/// # impl tablo_core::schema::OptionSource for AuthorResource {
/// #     type Model = Author;
/// #     fn scoped_query(_cx: &topcoat::context::Cx)
/// #         -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Author>>>
/// #     {
/// #         Ok(toasty::stmt::Query::all())
/// #     }
/// # }
/// # use tablo_core::{Field, Options};
/// Field::text(User::fields().name()).placeholder("Ada Lovelace"); // TextField
/// Field::text(User::fields().email()).email().unique();
/// Field::text(User::fields().age()); // typed: an `i64` column
/// Field::text(Post::fields().body()).multiline(6);
/// Field::choice(Post::fields().status()).options(Status::options()); // ChoiceField
/// Field::choice(Post::fields().author_id())
///     .relationship::<AuthorResource>(|a: &Author| a.name.clone());
/// Field::file(Doc::fields().path()); // FileField
/// Field::toggle(Post::fields().featured()); // CustomField
/// ```
///
/// ```compile_fail
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # fn main() {
/// tablo_core::Field::text(User::fields().name()).options(["admin", "member"]);
/// # }
/// ```
pub struct Field {
    name: String,
    label: String,
    required: bool,
    /// Whether the column stores NULL for an empty submission, which a unique
    /// index admits any number of times.
    nullable: bool,
    /// The email and scalar-parse rules, with their messages.
    rules: Rules,
    control: ControlKind,
    /// The declaration error when the lens binds no column.
    misdeclared: Option<crate::DeclarationErrorKind>,
}

/// The control a [`Field`] renders, with what only that control declares.
pub(crate) enum ControlKind {
    /// A one-line `<input>`, or a `<textarea>` when `rows` is set.
    Text(TextControl),
    /// A `<select>` over static options or a relationship's rows.
    Choice(ChoiceControl),
    /// A file input storing the uploaded file's path.
    File,
    /// An app's control, or the built-in [`Toggle`].
    Custom(Arc<dyn Control>),
}

impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let control = match &self.control {
            ControlKind::Text(_) => "text",
            ControlKind::Choice(_) => "choice",
            ControlKind::File => "file",
            ControlKind::Custom(_) => "custom",
        };
        f.debug_struct("Field")
            .field("name", &self.name)
            .field("label", &self.label)
            .field("required", &self.required)
            .field("control", &control)
            .finish()
    }
}

impl Field {
    fn bound<M, T>(lens: ResolvedLens<M, T>, rules: Rules, control: ControlKind) -> Self {
        Self {
            name: lens.name,
            label: lens.label,
            required: !lens.nullable,
            nullable: lens.nullable,
            rules,
            control,
            misdeclared: lens.misdeclared,
        }
    }

    /// Binds any [`FormScalar`] column, storing the type's own spelling and probing uniqueness
    /// through the lens.
    pub fn text<M, T>(lens: impl Into<Path<M, T>>) -> TextField
    where
        M: toasty::schema::Model,
        T: FormScalar + toasty::stmt::IntoExpr<T> + 'static,
    {
        let lens = ResolvedLens::of(lens);
        let control = TextControl::new::<M, T>(lens.path.clone(), lens.unique);
        TextField(Self::bound(
            lens,
            Rules::new().scalar::<T>(),
            ControlKind::Text(control),
        ))
    }

    /// The text field `#[derive(EmbeddedForm)]` renders for a leaf.
    #[doc(hidden)]
    pub fn embedded_leaf<M, T>(path: Path<M, T>) -> TextField
    where
        M: toasty::schema::Model,
        T: FormScalar,
    {
        let lens = ResolvedLens::of(path);
        TextField(Self::bound(
            lens,
            Rules::new().scalar::<T>(),
            ControlKind::Text(TextControl::leaf::<T>()),
        ))
    }

    /// A choice field over any column, with options from [`options`](ChoiceField::options) or
    /// [`relationship`](ChoiceField::relationship).
    pub fn choice<M, T>(lens: impl Into<Path<M, T>>) -> ChoiceField
    where
        M: toasty::schema::Model,
    {
        ChoiceField(Self::bound(
            ResolvedLens::of(lens),
            Rules::new(),
            ControlKind::Choice(ChoiceControl::default()),
        ))
    }

    /// A file field over a `String` column holding the uploaded path, rendering no `value`
    /// attribute.
    pub fn file<M>(lens: impl Into<Path<M, String>>) -> FileField
    where
        M: toasty::schema::Model,
    {
        FileField(Self::bound(
            ResolvedLens::of(lens),
            Rules::new(),
            ControlKind::File,
        ))
    }

    /// A checkbox over a `bool` column that submits `false` when unchecked.
    pub fn toggle<M>(lens: impl Into<Path<M, bool>>) -> CustomField
    where
        M: toasty::schema::Model,
    {
        Self::custom(lens, Toggle).optional()
    }

    /// A field over any [`FormScalar`] column, rendered by an app's [`Control`].
    pub fn custom<M, T>(lens: impl Into<Path<M, T>>, control: impl Control + 'static) -> CustomField
    where
        M: toasty::schema::Model,
        T: FormScalar,
    {
        CustomField(Self::bound(
            ResolvedLens::of(lens),
            Rules::new().scalar::<T>(),
            ControlKind::Custom(Arc::new(control)),
        ))
    }

    /// The variant control of an embedded enum, never required since an empty submit answers with
    /// the payload fallback.
    pub(crate) fn discriminant(name: String, variants: Vec<(String, String)>) -> Self {
        Self {
            label: capitalize(&name),
            name,
            required: false,
            nullable: true,
            rules: Rules::new(),
            control: ControlKind::Choice(ChoiceControl {
                discriminant: true,
                options: variants,
                ..ChoiceControl::default()
            }),
            misdeclared: None,
        }
    }

    pub(crate) fn misdeclared(&self) -> Option<&crate::DeclarationErrorKind> {
        self.misdeclared.as_ref()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn label_str(&self) -> &str {
        &self.label
    }

    pub(crate) fn as_choice(&self) -> Option<&ChoiceControl> {
        match &self.control {
            ControlKind::Choice(choice) => Some(choice),
            _ => None,
        }
    }

    /// Probes uniqueness with the submission parsed through the lens.
    pub(crate) fn eq_filter(&self, value: &str) -> Option<toasty::stmt::Expr<bool>> {
        match &self.control {
            ControlKind::Text(text) => text.eq_filter(value),
            _ => None,
        }
    }

    pub(crate) fn is_file(&self) -> bool {
        matches!(self.control, ControlKind::File)
    }

    pub(crate) fn is_unique(&self) -> bool {
        matches!(&self.control, ControlKind::Text(text) if text.unique)
    }

    /// Whether an empty submit fails and the control renders as required.
    pub(crate) fn is_required(&self) -> bool {
        self.required || (self.is_unique() && !self.nullable)
    }

    /// Validates a submitted value, each failure keyed by the field's own name.
    pub(crate) fn validate(&self, value: &str) -> Vec<FieldError> {
        self.rules
            .validate(&self.name, &self.label, self.is_required(), value)
    }

    /// The stored spelling of a validated submission.
    pub(crate) fn normalize(&self, value: &str) -> Result<String, String> {
        self.rules.normalize(value)
    }

    /// Whether a submitted choice matches its options.
    pub(crate) async fn validate_exists(&self, cx: &Cx, value: &str) -> Vec<String> {
        match &self.control {
            ControlKind::Choice(choice) => choice.validate_exists(cx, &self.label, value).await,
            _ => Vec::new(),
        }
    }

    /// Re-checks a relationship choice in the write's transaction.
    pub(crate) async fn recheck(
        &self,
        cx: &Cx,
        value: &str,
        ex: &mut dyn toasty::Executor,
    ) -> Vec<String> {
        match &self.control {
            ControlKind::Choice(choice) => choice.recheck(cx, &self.label, value, ex).await,
            _ => Vec::new(),
        }
    }

    /// Renders the field's control or its stored value.
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
        mode: Mode,
    ) -> Result<BoxView<'a>> {
        if mode == Mode::View && value.is_none() {
            debug_assert!(
                false,
                "view field `{}` has no value: neither `view_values` nor the record form's \
                 `hydrate` supplies its key",
                self.name
            );
            return render_value(cx, &self.label, Some("(missing)"), ValueKind::Prose);
        }
        match &self.control {
            ControlKind::Text(text) => self.render_text(text, cx, value, error, mode),
            ControlKind::Choice(choice) => {
                Box::pin(self.render_choice(choice, cx, value, error, mode)).await
            }
            ControlKind::File => self.render_file(cx, value, error, mode),
            ControlKind::Custom(control) => {
                self.render_custom(control.as_ref(), cx, value, error, mode)
            }
        }
    }

    /// Renders an app [`Control`] in its shared chrome.
    fn render_custom<'a>(
        &self,
        control: &dyn Control,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
        mode: Mode,
    ) -> Result<BoxView<'a>> {
        if mode == Mode::View {
            let shown = control.display(cx, value.unwrap_or_default());
            return render_value_view(cx, &self.label, shown);
        }
        let required = self.is_required();
        let chrome = FieldChrome::new(&self.name, error, None);
        let input = ControlInput::new(
            &self.name,
            value,
            required,
            chrome.aria_invalid() == "true",
            chrome.described_by(),
        );
        let rendered = control.render(cx, input);
        render_field(cx, &chrome, &self.label, required, attributes! {}, rendered)
    }
}

/// How a read-only value is presented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueKind {
    /// Wrapping text: a title, a body, an address.
    Prose,
    /// A stored path — no spaces to break at.
    Machine,
}

/// The read-only half of a field: the label with the record's stored value under it.
fn render_value<'a>(
    cx: &'a Cx,
    label: &str,
    value: Option<&str>,
    kind: ValueKind,
) -> Result<BoxView<'a>> {
    let text = value.unwrap_or_default().to_string();
    let value_class = match kind {
        ValueKind::Prose => "text-sm text-foreground wrap-anywhere whitespace-pre-wrap",
        ValueKind::Machine => "text-sm text-foreground font-mono break-all whitespace-pre-wrap",
    };
    let value = view! { cx => <div class=(value_class)>(text)</div> }.boxed();
    render_value_view(cx, label, value)
}

/// The field chrome around a read-only value that is not a plain string.
fn render_value_view<'a>(cx: &'a Cx, label: &str, value: BoxView<'a>) -> Result<BoxView<'a>> {
    let label = label.to_string();
    Ok(view! {
        cx =>
        ui_field(
            attrs: attributes! { class="ac-field" },
            ui_field_content(
                ui_field_title((label))
                (value)
            )
        )
    }
    .boxed())
}

/// The validation state a form control renders.
pub(crate) struct FieldChrome {
    name: String,
    error_id: String,
    error_text: String,
    has_error: bool,
}

impl FieldChrome {
    pub(crate) fn new(name: &str, error: Option<&str>, fallback: Option<String>) -> Self {
        let has_error = error.is_some() || fallback.is_some();
        let error_text = match error {
            Some(message) if !message.is_empty() => message.to_string(),
            _ => fallback.unwrap_or_default(),
        };
        Self {
            name: name.to_string(),
            error_id: format!("{name}-error"),
            error_text,
            has_error,
        }
    }

    pub(crate) fn aria_invalid(&self) -> &'static str {
        if self.has_error { "true" } else { "false" }
    }

    pub(crate) fn described_by(&self) -> Option<String> {
        self.has_error.then(|| self.error_id.clone())
    }
}

/// The chrome every form control renders: wrapper, label, control, and error slot.
pub(crate) fn render_field<'a>(
    cx: &'a Cx,
    chrome: &FieldChrome,
    label: &str,
    required: bool,
    attributes: Attributes,
    control: BoxView<'a>,
) -> Result<BoxView<'a>> {
    let name = chrome.name.clone();
    let label_text = label.to_string();
    let has_error = chrome.has_error;
    let error_id = chrome.error_id.clone();
    let error_text = chrome.error_text.clone();
    let field_class = if has_error {
        "ac-field ac-field--error"
    } else {
        "ac-field"
    };
    Ok(view! {
        cx =>
        ui_field(
            attrs: attributes! {
                class=(field_class)
                data-invalid=(has_error.then_some("true"))
                (attributes)
            },
            ui_field_label(
                attrs: attributes! { for=(name) },
                (label_text)
                if required {
                    <span class="text-destructive" aria-hidden="true">"*"</span>
                }
            )
            (control)
            if has_error {
                ui_field_error(
                    attrs: attributes! { id=(error_id) class="ac-error" aria-live="polite" },
                    (error_text)
                )
            }
        )
    }
    .boxed())
}

#[cfg(test)]
mod test_support {
    pub(super) use crate::test_support::{DummyUser, cx};

    /// The opening tag carrying `needle` (attributes render unordered, topcoat#122).
    pub(super) fn tag_with<'h>(html: &'h str, needle: &str) -> &'h str {
        let at = html
            .find(needle)
            .unwrap_or_else(|| panic!("no {needle} in {html}"));
        opening_tag_at(html, html[..at].rfind('<').expect("its opening tag"))
    }

    /// Slices the opening tag at `start`, honouring quoting (topcoat#122).
    pub(super) fn opening_tag_at(html: &str, start: usize) -> &str {
        let mut quoted = false;
        for (offset, byte) in html.as_bytes()[start..].iter().enumerate() {
            match byte {
                b'"' => quoted = !quoted,
                b'>' if !quoted => return &html[start..start + offset],
                _ => {}
            }
        }
        panic!("unterminated tag at byte {start} in {html}");
    }

    /// The sorted attributes of the tag carrying `needle` (unordered, topcoat#122).
    pub(super) fn attributes_of(html: &str, needle: &str) -> Vec<String> {
        let mut quoted = false;
        let mut attrs: Vec<String> = Vec::new();
        let mut current = String::new();
        for ch in tag_with(html, needle).chars() {
            match ch {
                '"' => {
                    quoted = !quoted;
                    current.push(ch);
                }
                ch if ch.is_whitespace() && !quoted => {
                    if !current.is_empty() {
                        attrs.push(std::mem::take(&mut current));
                    }
                }
                ch => current.push(ch),
            }
        }
        if !current.is_empty() {
            attrs.push(current);
        }
        attrs.remove(0); // the tag name
        attrs.sort();
        attrs
    }

    /// A nullable FK, for the optional-by-default choice case.
    #[derive(Debug, toasty::Model)]
    pub(super) struct NullableRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        parent_id: Option<uuid::Uuid>,
    }

    /// A non-nullable foreign key, for the required-by-default FK choice.
    #[derive(Debug, toasty::Model)]
    pub(super) struct FkRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        author_id: uuid::Uuid,
    }
}

#[cfg(test)]
mod tests;
