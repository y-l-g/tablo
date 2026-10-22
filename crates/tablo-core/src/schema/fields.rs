//! One [`Field`] type whose control is text, choice, file, or an app's own [`Control`], built from
//! a path binding a column or an embedded leaf.

mod builders;
mod choice;
pub(crate) mod custom;
mod file;
mod placement;
mod read_only;
mod rich_text;
mod text;

use std::sync::Arc;

pub use builders::{
    ChoiceField, CustomField, FileField, IntoFormValue, IntoOptions, RepeaterField, RichTextField,
    TextField,
};
pub(crate) use choice::{ChoiceControl, Relationship, option_view};
pub(crate) use custom::{Control, ControlInput};
pub use custom::{Switch, Toggle};
pub(crate) use file::{is_linkable, is_rooted, stored_upload};
pub(crate) use placement::Placement;
pub(crate) use read_only::{read_only, value_cell};
use rich_text::RichTextControl;
use tablo_ui::{
    field as ui_field, field_description as ui_field_description, field_error as ui_field_error,
    field_label as ui_field_label,
};
pub(crate) use text::{Pattern, TextControl, password_reveal};
use toasty::stmt::{List, Path};
use topcoat::{Result, context::Cx, view::*};

use super::{condition::Condition, lenses::Binding, repeater::RepeaterControl};
use crate::{
    dom,
    naming::capitalize,
    schema::FormInput,
    values::{FieldError, FieldErrorKind, FormScalar, FormValue},
};

/// One form field binding a lens to the control editing it, offering only that control's modifiers
/// so a modifier on the wrong control does not compile.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct User {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     name: String,
/// #     email: String,
/// #     age: i64,
/// # }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Post {
/// #     #[key] #[auto] id: uuid::Uuid,
/// #     body: String,
/// #     status: String,
/// #     author_id: uuid::Uuid,
/// #     featured: bool,
/// # }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Doc { #[key] #[auto] id: uuid::Uuid, path: String }
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
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
/// Options belong to a choice: a text field has no `options` method.
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
    /// The help text under the control, set by `help`.
    help: Option<String>,
    /// When the control renders disabled and the server ignores what it posts.
    disabled: Disabled,
    /// The create form's initial value, as the control posts it, set by `default`.
    default: Option<FormValue>,
}

/// When a [`Field`] renders disabled, so the browser posts nothing for it and the server drops
/// a key posted anyway.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Disabled {
    #[default]
    Never,
    Always,
    /// On the edit form only: the create form takes the value, which no edit then changes.
    OnEdit,
}

/// The control a [`Field`] renders, with what only that control declares.
pub(crate) enum ControlKind {
    /// A one-line `<input>`, or a `<textarea>` when `rows` is set.
    Text(TextControl),
    /// A `<textarea>` of HTML, which the browser edits through a toolbar.
    RichText(RichTextControl),
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
            ControlKind::RichText(_) => "rich text",
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
            .field("help", &self.help)
            .field("disabled", &self.disabled)
            .field("default", &self.default)
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
            help: None,
            disabled: Disabled::Never,
            default: None,
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
        let control =
            TextControl::new::<M, T>(path, binding.unique(), binding.unique_with().to_vec());
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
    /// input read them into `key`'s rows, and a page handling its own post reads it with
    /// [`Schema::read`](crate::Schema::read); the items then parse with
    /// [`parse_items`](crate::schema::parse_items).
    pub fn repeater<M, T>(lens: impl Into<Path<M, List<T>>>) -> RepeaterField
    where
        M: toasty::schema::Model,
        T: FormInput,
    {
        RepeaterField::new(Self::bound(
            Binding::of::<M, List<T>>(&lens.into()),
            ControlKind::Repeater(RepeaterControl::new::<T>()),
        ))
    }

    /// A text field posting `name`, a key no column binds: a [`FormInput`](crate::FormInput)
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
    pub(crate) fn discriminant(binding: Binding, variants: Vec<(String, String)>) -> Self {
        let choice = ChoiceControl {
            discriminant: true,
            options: variants,
            ..ChoiceControl::default()
        };
        Self::bound(binding, ControlKind::Choice(choice))
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

    /// Whether the field is a masked text control, whose value never renders back.
    pub(crate) fn is_password(&self) -> bool {
        matches!(&self.control, ControlKind::Text(text) if text.password)
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

    /// The fields each unique index covering a unique text field holds besides it.
    pub(crate) fn unique_with(&self) -> Vec<&[String]> {
        match &self.control {
            ControlKind::Text(text) if text.unique => text.unique_with(),
            _ => Vec::new(),
        }
    }

    /// The condition showing the field, if any.
    pub(crate) fn condition(&self) -> Option<&Condition> {
        self.condition.as_ref()
    }

    /// Shows the field only while `condition` holds, as `visible_when` does.
    pub(crate) fn set_condition(&mut self, condition: Condition) {
        self.condition = Some(condition);
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
            // Its key holds a list, which no condition value spells.
            ControlKind::Choice(choice) if choice.is_multiple() => Some(false),
            ControlKind::Choice(choice) if !choice.is_relationship() => {
                Some(value.is_empty() || choice.label_of(value).is_some())
            }
            _ => None,
        }
    }

    /// Whether the control could never post `value`: a typed text field's type does not parse
    /// it, a choice over static options or a checkbox does not offer it, or it is a list where
    /// the control posts one value, or one value where it posts a list.
    pub(crate) fn refuses(&self, value: &FormValue) -> bool {
        match (&self.control, value) {
            (ControlKind::Choice(choice), FormValue::List(items)) if choice.is_multiple() => {
                !choice.is_relationship()
                    && items.iter().any(|item| choice.label_of(item).is_none())
            }
            (ControlKind::Choice(choice), _) if choice.is_multiple() => true,
            (ControlKind::Text(text), FormValue::Text(value)) => {
                !value.trim().is_empty() && !text.parses(value)
            }
            (_, FormValue::Text(value)) => self.can_post(value) == Some(false),
            _ => true,
        }
    }

    /// Whether the control renders as required.
    pub(crate) fn is_required(&self) -> bool {
        self.required
    }

    /// Whether the control renders disabled, on the edit form when `editing`.
    pub(crate) fn is_disabled(&self, editing: bool) -> bool {
        match self.disabled {
            Disabled::Never => false,
            Disabled::Always => true,
            Disabled::OnEdit => editing,
        }
    }

    /// Whether the control renders disabled where `placement` puts it.
    pub(crate) fn is_disabled_at(&self, placement: &Placement<'_>) -> bool {
        placement.locked || self.is_disabled(placement.editing)
    }

    /// Whether the control renders disabled on the create form as well as the edit form.
    pub(crate) fn is_always_disabled(&self) -> bool {
        self.disabled == Disabled::Always
    }

    /// The create form's initial value, as the control posts it.
    pub(crate) fn default_value(&self) -> Option<&FormValue> {
        self.default.as_ref()
    }

    /// The validation state and help text the control renders with.
    fn chrome(
        &self,
        id: String,
        error: Option<&FieldError>,
        fallback: Option<String>,
    ) -> FieldChrome {
        let mut chrome = FieldChrome::new(id, error, self.label_str(), fallback);
        chrome.help = self.help.clone();
        chrome
    }

    /// Renders the control required when its record-form field has no blank answer.
    pub(crate) fn set_required(&mut self, required: bool) {
        self.required = required;
    }

    /// Whether the bound column stores NULL, which a unique index admits any number of times.
    pub(crate) fn is_nullable(&self) -> bool {
        self.binding.nullable()
    }

    /// What is wrong with the rules the field's text control declares, reported under `field`:
    /// one that cannot work, or a default its type does not parse or that breaks one.
    pub(crate) fn rule_errors(&self, field: &str) -> Vec<crate::DeclarationErrorKind> {
        let (mut faults, text) = match &self.control {
            ControlKind::Text(text) => (text.rule_faults(), Some(text)),
            ControlKind::RichText(rich) => (rich.rule_faults(), None),
            _ => return Vec::new(),
        };
        if let Some(value) = self.default_value() {
            let spelled = value.as_text().unwrap_or_default().trim();
            if !spelled.is_empty() && text.is_some_and(|text| !text.parses(spelled)) {
                faults.push(crate::FieldRuleFault::UnparsedDefault(spelled.to_string()));
            } else if self.check(value, &jiff::tz::TimeZone::UTC).is_some() {
                faults.push(crate::FieldRuleFault::RefusedDefault(spelled.to_string()));
            }
        }
        faults
            .into_iter()
            .map(|fault| crate::DeclarationErrorKind::FieldRule {
                field: field.to_string(),
                fault,
            })
            .collect()
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

    /// Whether a submitted choice matches the options its parent's value `parent` offers; a
    /// multiple choice checks only the keys its stored value `stored` does not hold.
    pub(crate) async fn validate_exists(
        &self,
        cx: &Cx,
        value: &FormValue,
        parent: Option<&str>,
        stored: Option<&FormValue>,
    ) -> Vec<String> {
        match &self.control {
            ControlKind::Choice(choice) => {
                choice
                    .validate_exists(cx, self.label_str(), value, parent, stored)
                    .await
            }
            _ => Vec::new(),
        }
    }

    /// Re-checks a relationship choice in the write's transaction; a multiple choice re-checks
    /// only the keys its stored value `stored` does not hold.
    pub(crate) async fn recheck(
        &self,
        cx: &Cx,
        value: &FormValue,
        parent: Option<&str>,
        stored: Option<&FormValue>,
        ex: &mut dyn toasty::Executor,
    ) -> Vec<String> {
        match &self.control {
            ControlKind::Choice(choice) => {
                choice
                    .recheck(cx, self.label_str(), value, parent, stored, ex)
                    .await
            }
            _ => Vec::new(),
        }
    }

    /// A multiple choice's `value` with the stored keys it drops that the user cannot view put
    /// back, or `None` when it drops none.
    pub(crate) async fn keep_unseen(
        &self,
        cx: &Cx,
        value: &FormValue,
        stored: &FormValue,
        ex: &mut dyn toasty::Executor,
    ) -> Option<FormValue> {
        match &self.control {
            ControlKind::Choice(choice) => choice.keep_unseen(cx, value, stored, ex).await,
            _ => None,
        }
    }

    /// Renders the field's control where `placement` puts it.
    pub(crate) async fn render_under<'a>(
        &self,
        cx: &'a Cx,
        value: Option<&FormValue>,
        error: Option<&FieldError>,
        placement: Placement<'_>,
    ) -> Result<BoxView<'a>> {
        let id = placement.id(self.name());
        let disabled = self.is_disabled_at(&placement);
        let text = value.and_then(FormValue::as_text);
        match &self.control {
            ControlKind::Text(control) => self.render_text(control, cx, text, error, id, disabled),
            ControlKind::RichText(rich) => {
                self.render_rich_text(rich, cx, text, error, id, disabled)
            }
            ControlKind::Choice(choice) => {
                Box::pin(self.render_choice(choice, cx, value, error, id, placement)).await
            }
            ControlKind::File => self.render_file(cx, text, error, id, disabled),
            ControlKind::Custom(control) => {
                self.render_custom(control.as_ref(), cx, text, error, id, disabled)
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
        error: Option<&FieldError>,
        id: String,
        disabled: bool,
    ) -> Result<BoxView<'a>> {
        let required = self.is_required() && !disabled;
        let chrome = self.chrome(id, error, None);
        let input = ControlInput::new(
            self.name(),
            &chrome.id,
            value,
            required,
            chrome.aria_invalid() == "true",
            chrome.described_by(),
        )
        .disabled(disabled);
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

/// The validation state a form control renders.
pub(crate) struct FieldChrome {
    /// The control's DOM id, which the label names.
    pub(crate) id: String,
    error_id: String,
    error_text: String,
    /// Why the control renders an error, `None` when it renders none.
    error_kind: Option<&'static str>,
    /// The help text under the control.
    help: Option<String>,
}

/// The `data-error` an error slot carries, naming why the control was refused: a test reads it
/// instead of the sentence.
pub(crate) fn error_kind(error: &FieldError) -> &'static str {
    match error.kind {
        FieldErrorKind::Required => dom::error::REQUIRED,
        FieldErrorKind::Invalid(_) => dom::error::INVALID,
    }
}

impl FieldChrome {
    /// The chrome of the control labelled `label`, refused with `error`. `fallback` is the
    /// message of a choice whose options the user may not read: its slot reads unavailable
    /// whatever refused the submission, and shows `fallback` when nothing did.
    pub(crate) fn new(
        id: String,
        error: Option<&FieldError>,
        label: &str,
        fallback: Option<String>,
    ) -> Self {
        let message = error.map(|error| error.message(label));
        let (error_text, error_kind) = match (message, fallback) {
            (Some(message), Some(_)) if !message.is_empty() => {
                (message, Some(dom::error::UNAVAILABLE))
            }
            (Some(message), None) => (message, error.map(error_kind)),
            (_, Some(fallback)) => (fallback, Some(dom::error::UNAVAILABLE)),
            (None, None) => (String::new(), None),
        };
        Self {
            error_id: format!("{id}-error"),
            id,
            error_text,
            error_kind,
            help: None,
        }
    }

    fn has_error(&self) -> bool {
        self.error_kind.is_some()
    }

    fn help_id(&self) -> String {
        format!("{}-description", self.id)
    }

    pub(crate) fn aria_invalid(&self) -> &'static str {
        if self.has_error() { "true" } else { "false" }
    }

    /// The ids of the help text and the error, whichever render.
    pub(crate) fn described_by(&self) -> Option<String> {
        let ids: Vec<String> = [
            self.help.is_some().then(|| self.help_id()),
            self.has_error().then(|| self.error_id.clone()),
        ]
        .into_iter()
        .flatten()
        .collect();
        (!ids.is_empty()).then(|| ids.join(" "))
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
    let has_error = chrome.has_error();
    let error_kind = chrome.error_kind;
    let error_id = chrome.error_id.clone();
    let error_text = chrome.error_text.clone();
    let help = chrome.help.clone();
    let help_id = chrome.help_id();
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
            if let Some(help) = help {
                ui_field_description(attrs: attributes! { id=(help_id) }, (help))
            }
            if has_error {
                ui_field_error(
                    attrs: attributes! {
                        id=(error_id)
                        class="ac-error"
                        aria-live="polite"
                        data-error=(error_kind)
                    },
                    (error_text)
                )
            }
        )
    }
    .boxed())
}

#[cfg(test)]
mod test_support {
    pub(super) use crate::test_support::DummyUser;
}

#[cfg(test)]
mod tests;
