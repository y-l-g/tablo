//! One [`Field`] type whose control is text, choice, file, or an app's own [`Control`], built from
//! a path binding a column or an embedded leaf.

mod builders;
mod choice;
pub(crate) mod custom;
mod file;
mod text;

use std::sync::Arc;

pub use builders::{ChoiceField, CustomField, FileField, IntoOptions, RepeaterField, TextField};
pub(crate) use choice::{ChoiceControl, option_view};
pub use custom::Toggle;
pub(crate) use custom::{Control, ControlInput};
pub(crate) use file::stored_upload;
use tablo_ui::{
    field as ui_field, field_content as ui_field_content, field_error as ui_field_error,
    field_label as ui_field_label, field_title as ui_field_title,
};
pub(crate) use text::TextControl;
use toasty::stmt::{List, Path};
use topcoat::{Result, context::Cx, view::*};

use super::{
    condition::Condition,
    lenses::{Binding, FieldResolver},
    repeater::{RepeaterControl, RepeaterItem},
    validation::is_email,
};
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
/// # impl tablo_core::extend::OptionSource for AuthorResource {
/// #     type Model = Author;
/// #     fn scoped_query(_cx: &topcoat::context::Cx)
/// #         -> topcoat::Result<toasty::stmt::Query<toasty::stmt::List<Author>>>
/// #     {
/// #         Ok(toasty::stmt::Query::all())
/// #     }
/// #     fn label(_cx: &topcoat::context::Cx, author: &Author) -> String {
/// #         author.name.clone()
/// #     }
/// # }
/// # use tablo_core::{Field, Options, lens};
/// Field::text(lens!(User.name)).placeholder("Ada Lovelace"); // TextField
/// Field::text(lens!(User.email)).email().unique();
/// Field::text(lens!(User.age)); // typed: an `i64` column
/// Field::text(lens!(Post.body)).multiline(6);
/// Field::choice(lens!(Post.status)).options(Status::options()); // ChoiceField
/// Field::choice(lens!(Post.author_id)).relationship::<AuthorResource>();
/// Field::file(lens!(Doc.path)); // FileField
/// Field::toggle(lens!(Post.featured)); // CustomField
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
    binding: Binding,
    /// The declared label, over the binding's.
    label: Option<String>,
    /// Whether the control renders as required: the panel sets it from the record form, and an
    /// embedded value's derive from its own fields.
    required: bool,
    control: ControlKind,
    /// The condition showing the field, set by `visible_when`.
    condition: Option<Condition>,
    /// Whether the control is a checkbox, which a condition follows through `checked`.
    checkbox: bool,
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
    /// A row of an item's controls per value of a `#[document]` list.
    Repeater(RepeaterControl),
}

impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let control = match &self.control {
            ControlKind::Text(_) => "text",
            ControlKind::Choice(_) => "choice",
            ControlKind::File => "file",
            ControlKind::Custom(_) => "custom",
            ControlKind::Repeater(_) => "repeater",
        };
        f.debug_struct("Field")
            .field("name", &self.name())
            .field("label", &self.label_str())
            .field("required", &self.required)
            .field("control", &control)
            .field("condition", &self.condition)
            .finish()
    }
}

impl Field {
    fn bound(binding: Binding, control: ControlKind) -> Self {
        Self {
            binding,
            label: None,
            required: false,
            control,
            condition: None,
            checkbox: false,
        }
    }

    /// Binds any [`FormScalar`] column, storing the type's own spelling and probing uniqueness
    /// through the lens.
    pub fn text<M, T>(lens: impl Into<Path<M, T>>) -> TextField
    where
        M: toasty::schema::Model,
        T: FormScalar + toasty::stmt::IntoExpr<T> + 'static,
    {
        let path: Path<M, T> = lens.into();
        let binding = Binding::of(&path);
        let control = TextControl::new::<M, T>(path, binding.unique());
        TextField::new(Self::bound(binding, ControlKind::Text(control)))
    }

    /// The text field `#[derive(EmbeddedForm)]` renders for a leaf.
    #[doc(hidden)]
    pub fn embedded_leaf<M, T>(path: Path<M, T>) -> TextField
    where
        M: toasty::schema::Model,
        T: FormScalar,
    {
        TextField::new(Self::bound(
            Binding::of(&path),
            ControlKind::Text(TextControl::leaf::<T>()),
        ))
    }

    /// A choice field over any column, with options from [`options`](ChoiceField::options) or
    /// [`relationship`](ChoiceField::relationship).
    pub fn choice<M, T>(lens: impl Into<Path<M, T>>) -> ChoiceField
    where
        M: toasty::schema::Model,
    {
        ChoiceField::new(Self::bound(
            Binding::of::<M, T>(&lens.into()),
            ControlKind::Choice(ChoiceControl::default()),
        ))
    }

    /// A file field over a `String` column holding the uploaded path, rendering no `value`
    /// attribute.
    pub fn file<M>(lens: impl Into<Path<M, String>>) -> FileField
    where
        M: toasty::schema::Model,
    {
        FileField::new(Self::bound(
            Binding::of::<M, String>(&lens.into()),
            ControlKind::File,
        ))
    }

    /// A checkbox over a `bool` column that submits `false` when unchecked.
    pub fn toggle<M>(lens: impl Into<Path<M, bool>>) -> CustomField
    where
        M: toasty::schema::Model,
    {
        let mut field = Self::custom(lens, Toggle);
        field.0.checkbox = true;
        field
    }

    /// A field over any [`FormScalar`] column, rendered by an app's [`Control`].
    pub fn custom<M, T>(lens: impl Into<Path<M, T>>, control: impl Control + 'static) -> CustomField
    where
        M: toasty::schema::Model,
        T: FormScalar,
    {
        CustomField::new(Self::bound(
            Binding::of::<M, T>(&lens.into()),
            ControlKind::Custom(Arc::new(control)),
        ))
    }

    /// A row of `T`'s controls per item of a `#[document]` list of `T`, which the browser adds,
    /// removes and moves: a record form's `#[form(repeat)]` control.
    ///
    /// Each row posts its item's keys under `{key}.{row}.`. A resource's form and an action's
    /// input fold them into `key` before reading the submission, and a page handling its own post
    /// folds it with [`Schema::fold_repeaters`](crate::Schema::fold_repeaters); the items then
    /// read with [`parse_items`](crate::schema::parse_items).
    pub fn repeater<M, T>(lens: impl Into<Path<M, List<T>>>) -> RepeaterField
    where
        M: toasty::schema::Model,
        T: RepeaterItem,
    {
        RepeaterField::new(Self::bound(
            Binding::of::<M, List<T>>(&lens.into()),
            ControlKind::Repeater(RepeaterControl::new::<T>()),
        ))
    }

    /// A text field posting `name`, a key no column binds: an [`ActionInput`](crate::ActionInput)
    /// field. `T` picks the input type, as a column's type does for [`Field::text`].
    pub fn text_input<T: FormScalar>(name: impl Into<String>) -> TextField {
        TextField::new(Self::bound(
            Self::named(name.into()),
            ControlKind::Text(TextControl::leaf::<T>()),
        ))
    }

    /// A choice field posting `name`, a key no column binds, with options from
    /// [`options`](ChoiceField::options).
    pub fn choice_input(name: impl Into<String>) -> ChoiceField {
        ChoiceField::new(Self::bound(
            Self::named(name.into()),
            ControlKind::Choice(ChoiceControl::default()),
        ))
    }

    /// A checkbox posting `name`, a key no column binds, that submits `false` when unchecked.
    pub fn toggle_input(name: impl Into<String>) -> CustomField {
        let mut field = Self::bound(
            Self::named(name.into()),
            ControlKind::Custom(Arc::new(Toggle)),
        );
        field.checkbox = true;
        CustomField::new(field)
    }

    /// A binding for `name`, labelled as a column of that name would be.
    fn named(name: String) -> Binding {
        let label = capitalize(&name);
        Binding::named(name, label)
    }

    /// The variant control of an embedded enum, never required since an empty submit reads the
    /// variant from the payload.
    pub(crate) fn discriminant(name: String, variants: Vec<(String, String)>) -> Self {
        Self {
            binding: Binding::named(name.clone(), capitalize(&name)),
            label: None,
            required: false,
            control: ControlKind::Choice(ChoiceControl {
                discriminant: true,
                options: variants,
                ..ChoiceControl::default()
            }),
            condition: None,
            checkbox: false,
        }
    }

    /// Bind an embedded path through `resolver`'s app schema.
    pub(crate) fn bind(&self, resolver: &FieldResolver) {
        self.binding.bind(resolver);
    }

    pub(crate) fn misdeclared(&self) -> Option<crate::DeclarationErrorKind> {
        self.binding.misdeclared()
    }

    /// The key the field posts: the storage column its path names.
    pub fn name(&self) -> &str {
        self.binding.name()
    }

    pub(crate) fn label_str(&self) -> &str {
        self.label.as_deref().unwrap_or(self.binding.label())
    }

    /// Whether the field is a choice declaring neither options nor a relationship.
    pub(crate) fn offers_nothing(&self) -> bool {
        self.as_choice().is_some_and(ChoiceControl::offers_nothing)
    }

    pub(crate) fn as_repeater(&self) -> Option<&RepeaterControl> {
        match &self.control {
            ControlKind::Repeater(repeater) => Some(repeater),
            _ => None,
        }
    }

    /// Posts under `prefix`: a repeater row's control, keeping its label.
    pub(crate) fn prefix(&mut self, prefix: &str) {
        let label = self.label_str().to_string();
        self.binding = Binding::named(format!("{prefix}{}", self.name()), label);
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

    /// The condition showing the field, if any.
    pub(crate) fn condition(&self) -> Option<&Condition> {
        self.condition.as_ref()
    }

    /// Whether the control is a checkbox.
    pub(crate) fn is_checkbox(&self) -> bool {
        self.checkbox
    }

    /// Whether the control can post `value`: a checkbox posts `true` or `false`, and a choice over
    /// static options one of their values. `None` when the control does not say: a text field, a
    /// relationship, or an app's own control.
    pub(crate) fn can_post(&self, value: &str) -> Option<bool> {
        if self.checkbox {
            return Some(value == "true" || value == "false");
        }
        match &self.control {
            ControlKind::Choice(choice) if !choice.is_relationship() => {
                Some(value.is_empty() || choice.label_of(value).is_some())
            }
            _ => None,
        }
    }

    /// Whether the control renders as required.
    pub(crate) fn is_required(&self) -> bool {
        self.required
    }

    /// Renders the control required when its record-form field has no blank answer.
    pub(crate) fn set_required(&mut self, required: bool) {
        self.required = required;
    }

    /// Whether the bound column stores NULL, which a unique index admits any number of times.
    pub(crate) fn is_nullable(&self) -> bool {
        self.binding.nullable()
    }

    /// Refuses a non-empty submission the control's own rule refuses: an email field's address.
    pub(crate) fn check(&self, value: &str) -> Option<FieldError> {
        let value = value.trim();
        match &self.control {
            ControlKind::Text(text) if text.email && !value.is_empty() && !is_email(value) => {
                Some(FieldError::invalid(
                    self.name(),
                    format!("{} must be a valid email", self.label_str()),
                ))
            }
            _ => None,
        }
    }

    /// Whether two submissions spell the same stored value, as the unique check compares them.
    pub(crate) fn same_value(&self, a: &str, b: &str) -> bool {
        match &self.control {
            ControlKind::Text(text) => text.same_value(a, b),
            _ => a.trim() == b.trim(),
        }
    }

    /// The key of the field whose value narrows a dependent choice's options.
    pub(crate) fn parent_key(&self) -> Option<&str> {
        self.as_choice().and_then(ChoiceControl::parent_key)
    }

    /// Whether a submitted choice matches the options its parent's value `parent` offers.
    pub(crate) async fn validate_exists(
        &self,
        cx: &Cx,
        value: &str,
        parent: Option<&str>,
    ) -> Vec<String> {
        match &self.control {
            ControlKind::Choice(choice) => {
                choice
                    .validate_exists(cx, self.label_str(), value, parent)
                    .await
            }
            _ => Vec::new(),
        }
    }

    /// Re-checks a relationship choice in the write's transaction.
    pub(crate) async fn recheck(
        &self,
        cx: &Cx,
        value: &str,
        parent: Option<&str>,
        ex: &mut dyn toasty::Executor,
    ) -> Vec<String> {
        match &self.control {
            ControlKind::Choice(choice) => {
                choice
                    .recheck(cx, self.label_str(), value, parent, ex)
                    .await
            }
            _ => Vec::new(),
        }
    }

    /// Renders the field's control alone on its page, a dependent choice with no parent value.
    #[cfg(test)]
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
    ) -> Result<BoxView<'a>> {
        self.render_under(cx, value, error, Placement::default())
            .await
    }

    /// Renders the field's control where `placement` puts it.
    pub(crate) async fn render_under<'a>(
        &self,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
        placement: Placement<'_>,
    ) -> Result<BoxView<'a>> {
        let id = placement.id(self.name());
        match &self.control {
            ControlKind::Text(text) => self.render_text(text, cx, value, error, id),
            ControlKind::Choice(choice) => {
                Box::pin(self.render_choice(choice, cx, value, error, id, placement)).await
            }
            ControlKind::File => self.render_file(cx, value, error, id),
            ControlKind::Custom(control) => {
                self.render_custom(control.as_ref(), cx, value, error, id)
            }
            // Its node renders it, from the keys and errors of each row's controls.
            ControlKind::Repeater(_) => Ok(().boxed()),
        }
    }

    /// The stored `value` as a reader sees it: a choice's option label, else the value itself.
    /// `None` for a variant control whose value names no variant.
    pub(crate) fn read(&self, value: &str) -> Option<String> {
        match &self.control {
            ControlKind::Choice(choice) => {
                let stored = value.trim();
                match choice.label_of(stored) {
                    Some(label) => Some(label.to_string()),
                    None if choice.discriminant => None,
                    None => Some(stored.to_string()),
                }
            }
            _ => Some(value.to_string()),
        }
    }

    /// Renders the label over the stored `value`, [read](Self::read) rather than edited: an
    /// embedded value's leaf or variant on a detail page.
    pub(crate) fn display<'a>(&self, cx: &'a Cx, value: &str) -> BoxView<'a> {
        match self.read(value) {
            Some(text) => read_only(cx, self.label_str(), value_cell(cx, &text)),
            None => ().boxed(),
        }
    }

    /// Renders an app [`Control`] in its shared chrome.
    fn render_custom<'a>(
        &self,
        control: &dyn Control,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&str>,
        id: String,
    ) -> Result<BoxView<'a>> {
        let required = self.is_required();
        let chrome = FieldChrome::new(id, error, None);
        let input = ControlInput::new(
            self.name(),
            &chrome.id,
            value,
            required,
            chrome.aria_invalid() == "true",
            chrome.described_by(),
        );
        let rendered = control.render(cx, input);
        render_field(
            cx,
            &chrome,
            self.label_str(),
            required,
            attributes! {},
            rendered,
        )
    }
}

/// A read-only value: `label` over `value`, which wraps as text. A detail page's entry, and an
/// embedded leaf on one.
pub(crate) fn read_only<'a>(cx: &'a Cx, label: &str, value: BoxView<'a>) -> BoxView<'a> {
    let label = label.to_string();
    view! {
        cx =>
        ui_field(
            attrs: attributes! { class="ac-field" },
            ui_field_content(
                ui_field_title((label))
                <div class="text-sm text-foreground wrap-anywhere whitespace-pre-wrap">
                    (value)
                </div>
            )
        )
    }
    .boxed()
}

/// A record's value as read-only text: a blank value reads as a muted dash.
pub(crate) fn value_cell<'a>(cx: &'a Cx, text: &str) -> BoxView<'a> {
    if text.trim().is_empty() {
        return view! { cx => <span class="text-muted-foreground">"—"</span> }.boxed();
    }
    let text = text.to_string();
    view! { cx => (text) }.boxed()
}

/// Where a field's control renders: under its parent's value, and in which form of its page.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Placement<'p> {
    /// The value the field's parent posts: a dependent choice offers the rows it selects.
    pub(crate) parent: Option<&'p str>,
    /// The prefix of the form's DOM ids, so several forms on one page never share one; empty
    /// for a form alone on its page.
    pub(crate) scope: &'p str,
    /// The URL a searchable choice fetches its options from; `None` for the resource's own
    /// `{list}/options`, which the browser derives from the page's URL.
    pub(crate) options: Option<&'p str>,
}

impl Placement<'_> {
    /// The DOM id of the control posting `name`.
    pub(crate) fn id(&self, name: &str) -> String {
        if self.scope.is_empty() {
            name.to_string()
        } else {
            format!("{}-{name}", self.scope)
        }
    }
}

/// The validation state a form control renders.
pub(crate) struct FieldChrome {
    /// The control's DOM id, which the label names.
    pub(crate) id: String,
    error_id: String,
    error_text: String,
    has_error: bool,
}

impl FieldChrome {
    pub(crate) fn new(id: String, error: Option<&str>, fallback: Option<String>) -> Self {
        let has_error = error.is_some() || fallback.is_some();
        let error_text = match error {
            Some(message) if !message.is_empty() => message.to_string(),
            _ => fallback.unwrap_or_default(),
        };
        Self {
            error_id: format!("{id}-error"),
            id,
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
    let id = chrome.id.clone();
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
                attrs: attributes! { for=(id) },
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
}

#[cfg(test)]
mod tests;
