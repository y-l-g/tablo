//! Field leaves: one [`Field`] type whose control is text, choice, file, or
//! an app's own [`Control`].
//!
//! Every constructor takes a [`ResolvedLens`], which is the single source of
//! truth for the field's key, label, and required default, so a field of any
//! kind binds a top-level column or an embedded leaf alike.

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
use topcoat::{Result, context::Cx, view::*};

use super::{
    lenses::{DeclCx, ResolvedLens, capitalize},
    tree::Mode,
    validation::Rules,
};
use crate::form::{FieldError, FormScalar};

/// One form field: a key, a label, the rules a submission meets, and the
/// control that edits it.
///
/// A constructor picks the control and returns that control's builder, which
/// offers only the modifiers the control has, so a modifier on the wrong
/// control does not compile. Each binds any lens a [`ResolvedLens`] accepts —
/// a column (`Post::fields().title()`) or an embedded leaf
/// (`ResolvedLens::new(dx, Post::fields().seo().title())`):
///
/// ```ignore
/// Field::text(User::fields().name()).placeholder("Ada Lovelace")   // TextField
/// Field::text(User::fields().email()).email().unique()
/// Field::text(User::fields().age())                  // typed: an `i64` column
/// Field::text(Post::fields().body()).multiline(6)
/// Field::choice(Post::fields().status()).options(Status::options()) // ChoiceField
/// Field::choice(Post::fields().author_id()).relationship::<AuthorResource>(..)
/// Field::file(Doc::fields().path())                                  // FileField
/// Field::toggle(Post::fields().featured())                           // CustomField
/// ```
///
/// Every builder has `label`, `required` and `optional`. `required` defaults
/// from the column's nullability: a non-nullable column is required, so an
/// empty submit fails inline instead of at the driver. `.optional()` opts out
/// and `.required()` opts back in.
///
/// A modifier on the wrong control does not compile: `options` is a choice
/// modifier, so it is not a method on a text field.
///
/// ```compile_fail
/// # #[derive(Debug, Clone, toasty::Model)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String }
/// # fn main() {
/// tablo_core::Field::text(User::fields().name()).options(["admin", "member"]);
/// # }
/// ```
///
/// A builder converts into a `Field` wherever a schema takes one
/// ([`IntoSchema`](super::IntoSchema)).
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
    /// Why the field's lens binds no column, when it does not
    /// ([`Schema::declaration_errors`](super::Schema::declaration_errors)).
    misdeclared: Option<String>,
}

/// The control a [`Field`] renders, with what only that control declares.
///
/// Text, choice, and file are the framework's own: the unique probe, the
/// relationship option endpoint, and the multipart parser read them.
/// Everything else is a [`Control`].
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
        // The probe and the loaders are closures with no useful Debug;
        // everything a reader needs is the field's identity and its control.
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
    /// The field every constructor starts from: the lens's key, label and
    /// required default, with `rules` and `control`.
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

    /// A text field over a column of any [`FormScalar`] type: `String`, a
    /// [`TypedValue`](crate::schema::TypedValue) type, or an `Option` of one.
    ///
    /// The control renders the stored value, and a typed column adds its
    /// spelling rule:
    ///
    /// - a submission the type refuses is an inline field error naming the input (`` `2024-13-01`
    ///   is not a valid timestamp ``);
    /// - what is stored is the type's own spelling of the parsed value, so a value re-submitted
    ///   unchanged is written back in the shape it was read.
    ///
    /// The control's `type` is the type's `INPUT_TYPE`: `datetime-local` for a
    /// `jiff::Timestamp`, whose stored instant renders in UTC and whose
    /// submission is read back as UTC, and `text` otherwise (`email` overrides
    /// it). `unique` defaults from the column's unique index (GH #183).
    ///
    /// `T` is [`IntoExpr`](toasty::stmt::IntoExpr) of itself so the unique
    /// probe compares the parsed value through the lens rather than its text.
    pub fn text<M, T>(lens: impl Into<ResolvedLens<M, T>>) -> TextField
    where
        M: toasty::schema::Model,
        T: FormScalar + toasty::stmt::IntoExpr<T> + 'static,
    {
        let lens = lens.into();
        let control = TextControl::new::<M, T>(lens.path.clone(), lens.unique);
        TextField(Self::bound(
            lens,
            Rules::new().scalar::<T>(),
            ControlKind::Text(control),
        ))
    }

    /// The text field `#[derive(EmbeddedForm)]` renders for a leaf: bound
    /// through the app schema, and bounded by `FormScalar` alone so a leaf of
    /// another type fails at the derive's `FormScalar` assertion.
    #[doc(hidden)]
    pub fn embedded_leaf<M, T>(dx: &DeclCx, path: toasty::stmt::Path<M, T>) -> TextField
    where
        M: toasty::schema::Model,
        T: FormScalar,
    {
        let lens = ResolvedLens::new(dx, path);
        TextField(Self::bound(
            lens,
            Rules::new().scalar::<T>(),
            ControlKind::Text(TextControl::leaf::<T>()),
        ))
    }

    /// A choice field over a column of any type, often a foreign key
    /// (`Post::fields().author_id()`).
    ///
    /// A bare choice validates presence only; its options come from
    /// [`options`](ChoiceField::options) or
    /// [`relationship`](ChoiceField::relationship), which also checks that a
    /// submitted key exists, tenant-aware.
    pub fn choice<M, T>(lens: impl Into<ResolvedLens<M, T>>) -> ChoiceField
    where
        M: toasty::schema::Model,
    {
        ChoiceField(Self::bound(
            lens.into(),
            Rules::new(),
            ControlKind::Choice(ChoiceControl::default()),
        ))
    }

    /// A file field over a `String` column holding the uploaded file's
    /// **path**, never its bytes.
    ///
    /// A form holding one renders `enctype="multipart/form-data"`, and the
    /// POST parser extracts the file part; where the bytes go is the app's
    /// decision, expressed by the [`Uploader`](crate::Uploader) installed with
    /// [`Panel::uploads`](crate::Panel::uploads). With none, the sanitized
    /// basename is stored. The file input renders no `value` attribute, which
    /// browsers ignore for security, so an edit form shows the stored path and
    /// a `clear_<field>` checkbox beside an empty input, and the input is
    /// required only while nothing is stored.
    pub fn file<M>(lens: impl Into<ResolvedLens<M, String>>) -> FileField
    where
        M: toasty::schema::Model,
    {
        FileField(Self::bound(lens.into(), Rules::new(), ControlKind::File))
    }

    /// A checkbox over a `bool` column: the built-in [`Toggle`].
    ///
    /// An unchecked box submits `false`, so the field is never empty and
    /// carries no required marker.
    pub fn toggle<M>(lens: impl Into<ResolvedLens<M, bool>>) -> CustomField
    where
        M: toasty::schema::Model,
    {
        Self::custom(lens, Toggle).optional()
    }

    /// A field over a column of any [`FormScalar`] type, rendered by an
    /// app's [`Control`].
    ///
    /// The field keeps the shared rules: the required default from the
    /// column's nullability, the type's parse rule, and the error slot. The
    /// control renders only the input.
    pub fn custom<M, T>(
        lens: impl Into<ResolvedLens<M, T>>,
        control: impl Control + 'static,
    ) -> CustomField
    where
        M: toasty::schema::Model,
        T: FormScalar,
    {
        CustomField(Self::bound(
            lens.into(),
            Rules::new().scalar::<T>(),
            ControlKind::Custom(Arc::new(control)),
        ))
    }

    /// The variant control of an embedded enum: a choice over its
    /// discriminant column, which the schema generates no lens for.
    ///
    /// Each option submits a variant's stored value and reads as its name. It
    /// is never required: an empty submit is "no variant named", which the
    /// value codec answers with its payload fallback, so refusing it would make
    /// that fallback unreachable.
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

    /// Why this field binds no column, when its lens refused to.
    pub(crate) fn misdeclared(&self) -> Option<&str> {
        self.misdeclared.as_deref()
    }

    /// The key the control posts.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The label the control renders.
    pub(crate) fn label_str(&self) -> &str {
        &self.label
    }

    /// The choice control, when this is a choice field.
    pub(crate) fn as_choice(&self) -> Option<&ChoiceControl> {
        match &self.control {
            ControlKind::Choice(choice) => Some(choice),
            _ => None,
        }
    }

    /// The equality expression the app-side unique check probes with: the
    /// submission parsed into the field's type, compared through its lens.
    /// `None` for a submission the type refuses, and for a field that is not
    /// text.
    pub(crate) fn eq_filter(&self, value: &str) -> Option<toasty::stmt::Expr<bool>> {
        match &self.control {
            ControlKind::Text(text) => text.eq_filter(value),
            _ => None,
        }
    }

    /// Whether this is a file field.
    pub(crate) fn is_file(&self) -> bool {
        matches!(self.control, ControlKind::File)
    }

    /// Whether this is a text field marked [`unique`](TextField::unique).
    pub(crate) fn is_unique(&self) -> bool {
        matches!(&self.control, ControlKind::Text(text) if text.unique)
    }

    /// Whether an empty submit fails validation and the control renders as
    /// required: `required`, or a unique text field over a non-nullable
    /// column (GH #189). `validate` and the render read it, so the rule and
    /// the marker cannot disagree.
    pub(crate) fn is_required(&self) -> bool {
        self.required || (self.is_unique() && !self.nullable)
    }

    /// Validate a raw submitted value against the field's rules, each failure
    /// keyed by the field's own name.
    pub(crate) fn validate(&self, value: &str) -> Vec<FieldError> {
        self.rules
            .validate(&self.name, &self.label, self.is_required(), value)
    }

    /// The stored spelling of a submission the caller has already validated:
    /// the scalar parse's spelling for a text field, the trimmed submission
    /// otherwise.
    ///
    /// A choice stores its trimmed submission because its presence rule and
    /// its option-existence check both read the trimmed value, so the trimmed
    /// value is the one that passed.
    pub(crate) fn normalize(&self, value: &str) -> Result<String, String> {
        self.rules.normalize(value)
    }

    /// Existence of a submitted choice among its options: a static option, or
    /// a relationship row the user may view. Empty for any other field.
    pub(crate) async fn validate_exists(&self, cx: &Cx, value: &str) -> Vec<String> {
        match &self.control {
            ControlKind::Choice(choice) => choice.validate_exists(cx, &self.label, value).await,
            _ => Vec::new(),
        }
    }

    /// Render the field: its control in `Mode::Form`, its stored value in
    /// `Mode::View`.
    ///
    /// A view field whose key the values do not carry renders `(missing)` and
    /// fails a `debug_assert!`: neither the resource's `view_values` nor its
    /// record form supplies the key, so the page would otherwise show a blank
    /// that reads as an empty value. It is the contract the list columns keep
    /// for an unloaded relation (ADR-0011).
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

    /// Render an app [`Control`]: its `display` on the detail page, its
    /// input inside the shared field chrome on a form.
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
///
/// Two shapes, because the difference is content, not styling: prose wraps at
/// spaces (and breaks a token too long for its line), and a stored path has no
/// spaces to break at, so it breaks anywhere and sets in mono.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueKind {
    /// Wrapping text: a title, a body, an address.
    Prose,
    /// A stored path — no spaces to break at.
    Machine,
}

/// The read-only half of a field: the label with the record's stored
/// value under it, no control and no validation slot.
///
/// Every control renders its view through this, so a detail page reads
/// uniformly. The label is the same `field_label` the form uses, inside the
/// same `field` family, so a field is recognisable across the two pages.
///
/// An empty value renders empty: a `String` column stores `""` and a NULL
/// hydrates as `""`, so the page cannot tell "no value" from an empty one.
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

/// The field chrome `render_value` puts around a rendered value, for
/// a field whose read-only value is not a plain string.
///
/// A file field renders its stored path as a link and supplies that view
/// here, so the label, the `field` family and the `ac-field` marker stay the
/// ones every other read-only field renders through.
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

/// The validation state a form control renders: the error id its
/// `aria-describedby` points at, the message its error slot shows, and whether
/// the field is invalid.
///
/// A field is invalid when it carries an error or when it has a `fallback`
/// message of its own — the relationship denial a choice surfaces on GET,
/// which has no `errors` entry yet.
pub(crate) struct FieldChrome {
    name: String,
    error_id: String,
    error_text: String,
    has_error: bool,
}

impl FieldChrome {
    pub(crate) fn new(name: &str, error: Option<&str>, fallback: Option<String>) -> Self {
        let has_error = error.is_some() || fallback.is_some();
        // An empty message is no message: the field's own fallback wording
        // renders instead, if it has one.
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

    /// The control's `aria-invalid`: `"true"` also colors the field's label.
    pub(crate) fn aria_invalid(&self) -> &'static str {
        if self.has_error { "true" } else { "false" }
    }

    /// The control's `aria-describedby`, pointing at the error slot while the
    /// field is invalid. Owned because a rendered view outlives this value.
    pub(crate) fn described_by(&self) -> Option<String> {
        self.has_error.then(|| self.error_id.clone())
    }
}

/// The chrome every form control renders: the `field` wrapper carrying
/// `ac-field` / `ac-field--error`, the label with the required marker, the
/// control, and the error slot.
///
/// `attributes` carries the extra wrapper attributes a control needs — the
/// choice option and filter hooks.
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
    pub(super) use crate::test_support::cx;
    #[derive(Debug, toasty::Model)]
    pub(super) struct DummyUser {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
        #[unique]
        email: String,
    }

    /// The whole opening tag carrying `needle` — how a test asserts on an
    /// element whose attributes render in no guaranteed order (topcoat#122)
    /// without depending on a marker attribute nothing consumes.
    pub(super) fn tag_with<'h>(html: &'h str, needle: &str) -> &'h str {
        let at = html
            .find(needle)
            .unwrap_or_else(|| panic!("no {needle} in {html}"));
        opening_tag_at(html, html[..at].rfind('<').expect("its opening tag"))
    }

    /// The opening tag that starts at `start`, sliced up to the `>` closing it.
    ///
    /// `Attributes` renders in no guaranteed order (topcoat#122), so a test
    /// locates a tag by whichever attribute it can and asserts on the whole
    /// tag. Quoting is honoured, so a `>` inside an attribute value (Tailwind
    /// selectors carry them) does not end the slice.
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

    /// The attributes of the opening tag carrying `needle`, sorted — quoting is
    /// honoured, so a Tailwind class value stays one token.
    ///
    /// `Attributes` renders in no guaranteed order (topcoat#122), so two
    /// renders of the same markup compare as sets of `name="value"` tokens.
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
