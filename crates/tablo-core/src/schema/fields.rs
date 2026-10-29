//! One [`Field`] type whose control is text, choice, file, or an app's own [`Control`], built from
//! a path binding a column or an embedded leaf.

mod availability;
mod builders;
mod check;
mod choice;
mod chrome;
pub(crate) mod custom;
pub(crate) mod file;
mod placement;
mod read_only;
mod render;
#[cfg(feature = "rich-text")]
mod rich_text;
mod text;

use std::sync::Arc;

pub(crate) use availability::{Disabled, Hidden};
pub use builders::{
    ChoiceField, CustomField, FileField, IntoFormValue, IntoOptions, RepeaterField, TextField,
};
pub(crate) use check::Items;
pub(crate) use choice::{ChoiceControl, Relationship, option_view};
pub(crate) use chrome::{FieldChrome, error_kind, render_field};
pub(crate) use custom::{Control, ControlInput};
pub use custom::{Switch, Toggle};
pub(crate) use file::{FileControl, is_linkable, is_rooted, stored_upload};
pub(crate) use placement::{Placement, Posting};
pub(crate) use read_only::{read_only, value_cell};
#[cfg(feature = "rich-text")]
use rich_text::RichTextControl;
#[cfg(feature = "rich-text")]
pub use rich_text::RichTextField;
pub(crate) use text::{Kind, Pattern, TextControl, password_reveal};
use toasty::stmt::{List, Path};
use topcoat::context::Cx;

use super::{condition::Condition, lenses::Binding, repeater::RepeaterControl};
use crate::{
    Text,
    schema::FormInput,
    values::{FieldError, FormScalar, FormValue},
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
/// # struct Doc { #[key] #[auto] id: uuid::Uuid, path: String, attachments: Vec<String> }
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
/// Field::files(lens!(Doc.attachments)); // FileField over a `Vec<String>`
/// Field::toggle(lens!(Post.featured)); // CustomField
/// ```
///
/// Options belong to a choice: a text field has no `options` method.
pub struct Field {
    binding: Binding,
    /// The declared label, over the binding's.
    label: Option<Text>,
    /// Whether the control renders as required: the panel sets it from the record form, and an
    /// embedded value's derive from its own fields.
    required: bool,
    control: ControlKind,
    /// The condition showing the field, set by `visible_when`.
    condition: Option<Condition>,
    /// Whether the control is a checkbox, which a condition follows through `checked`.
    checkbox: bool,
    /// Whether the checkbox must be checked, set by `accepted`.
    accepted: bool,
    /// The help text under the control, set by `help`.
    help: Option<Text>,
    /// When the control renders disabled and the server ignores what it posts.
    disabled: Disabled,
    /// The condition disabling the control, set by `disabled_when`.
    disabling: Option<Condition>,
    /// The forms that render nothing for the field, set by `hidden_on_create` and
    /// `hidden_on_edit`.
    hidden: Hidden,
    /// The create form's initial value, as the control posts it, set by `default`.
    default: Option<FormValue>,
}

/// The control a [`Field`] renders, with what only that control declares.
pub(crate) enum ControlKind {
    /// A one-line `<input>`, or a `<textarea>` when `rows` is set.
    Text(TextControl),
    /// A `<textarea>` of HTML, which the browser edits through a toolbar.
    #[cfg(feature = "rich-text")]
    RichText(RichTextControl),
    /// A `<select>` over static options or a relationship's rows.
    Choice(ChoiceControl),
    /// A file input storing the uploaded file's path.
    File(FileControl),
    /// An app's control, or the built-in [`Toggle`].
    Custom(Arc<dyn Control>),
    /// A row of an item's controls per value of a `#[document]` list.
    Repeater(RepeaterControl),
}

impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let control = match &self.control {
            ControlKind::Text(_) => "text",
            #[cfg(feature = "rich-text")]
            ControlKind::RichText(_) => "rich text",
            ControlKind::Choice(_) => "choice",
            ControlKind::File(_) => "file",
            ControlKind::Custom(_) => "custom",
            ControlKind::Repeater(_) => "repeater",
        };
        f.debug_struct("Field")
            .field("name", &self.name())
            .field("label", &self.label())
            .field("required", &self.required)
            .field("control", &control)
            .field("condition", &self.condition)
            .field("help", &self.help)
            .field("disabled", &self.disabled)
            .field("disabling", &self.disabling)
            .field("hidden", &self.hidden)
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
            accepted: false,
            help: None,
            disabled: Disabled::Never,
            disabling: None,
            hidden: Hidden::default(),
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
            ControlKind::File(FileControl::default()),
        ))
    }

    /// A file field over a `Vec<String>` column holding the uploaded paths, in the order the
    /// form lists them: a record form's `#[form(file)]` control on a `Vec<String>`.
    ///
    /// The input takes several files at once, which the submission adds after the stored ones,
    /// and each stored file has its own remove checkbox and, with JavaScript, buttons moving it:
    /// the list saves in the order its rows show. A resource's form reads the paths into `key`'s
    /// list, starting from the record's stored list: only a remove checkbox drops a path.
    /// A page handling its own post receives each chosen file's name under `key`, each row's path
    /// under `keep_<key>` in the order the rows post them, and each removed path under
    /// `clear_<key>`, and checks them against its record itself.
    /// [`FileField::max_files`] bounds the count; the extensions and size apply to each file.
    pub fn files<M>(lens: impl Into<Path<M, List<String>>>) -> FileField
    where
        M: toasty::schema::Model,
    {
        FileField::new(Self::bound(
            Binding::of::<M, List<String>>(&lens.into()),
            ControlKind::File(FileControl {
                multiple: true,
                ..FileControl::default()
            }),
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
    /// [`parse_items`](crate::schema::parse_items). [`RepeaterField`] has the rules.
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
        let label = Text::field(&name);
        Binding::named(name, label)
    }

    /// The variant control of an embedded enum, never required since an empty submit reads the
    /// variant from the payload.
    pub(crate) fn discriminant(binding: Binding, variants: Vec<(String, Text)>) -> Self {
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

    /// The declared label, else the binding's.
    pub(crate) fn label(&self) -> Text {
        self.label.clone().unwrap_or_else(|| self.binding.label())
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
        let label = self.label();
        self.binding = Binding::named(format!("{prefix}{}", self.name()), label);
        self.prefix_comparisons(prefix);
    }

    pub(crate) fn as_choice(&self) -> Option<&ChoiceControl> {
        match &self.control {
            ControlKind::Choice(choice) => Some(choice),
            _ => None,
        }
    }

    pub(crate) fn is_file(&self) -> bool {
        matches!(self.control, ControlKind::File(_))
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

    /// The create form's initial value, as the control posts it.
    pub(crate) fn default_value(&self) -> Option<&FormValue> {
        self.default.as_ref()
    }

    /// The validation state and help text the control renders with.
    fn chrome(
        &self,
        cx: &Cx,
        id: String,
        error: Option<&FieldError>,
        fallback: Option<String>,
    ) -> FieldChrome {
        let mut chrome = FieldChrome::new(cx, id, error, &self.label(), fallback);
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
    ) -> Vec<crate::Text> {
        match &self.control {
            ControlKind::Choice(choice) => {
                choice
                    .validate_exists(cx, &self.label(), value, parent, stored)
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
    ) -> Vec<crate::Text> {
        match &self.control {
            ControlKind::Choice(choice) => {
                choice
                    .recheck(cx, &self.label(), value, parent, stored, ex)
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
}

#[cfg(test)]
mod test_support {
    pub(super) use crate::test_support::DummyUser;
}

#[cfg(test)]
mod tests;
