//! The typed builders the [`Field`] constructors return, each offering only its control's modifiers
//! so a modifier on the wrong control does not compile.

use std::{marker::PhantomData, sync::Arc};

use super::{
    super::{
        IntoSchema, OptionSource, Retype, Schema,
        condition::{Condition, Watched},
    },
    ChoiceControl, Control, ControlKind, Field, Pattern, TextControl, choice,
};
use crate::values::{FormScalar, FormValue};

/// `label`, `name` and `visible_when`, shared by every builder.
macro_rules! base_modifiers {
    ($builder:ident) => {
        impl $builder {
            pub(super) fn new(field: Field) -> Self {
                Self(field, PhantomData)
            }
        }

        impl<F> $builder<F> {
            pub fn label(mut self, label: impl Into<String>) -> Self {
                self.0.label = Some(label.into());
                self
            }

            pub fn name(&self) -> &str {
                self.0.name()
            }

            /// Shows the field only while `watched` posts one of `values`: an option's value, or
            /// `true` for a checked toggle. A hidden field posts nothing, so an edit keeps its
            /// stored value and a create takes its blank answer; a field with no blank answer
            /// cannot be conditional. A field shows under one condition: a later call replaces an
            /// earlier one, so a field depending on two watched fields sits in a
            /// [`Section`](crate::Section) guarded by the other.
            ///
            /// ```rust
            /// # #[derive(Debug, Clone, toasty::Model)]
            /// # struct Customer {
            /// #     #[key] #[auto] id: uuid::Uuid,
            /// #     kind: String,
            /// #     vat_number: Option<String>,
            /// # }
            /// # use tablo_core::{Field, Schema};
            /// let kind = Field::choice(Customer::fields().kind()).options(["person", "company"]);
            /// let vat = Field::text(Customer::fields().vat_number()).visible_when(&kind, ["company"]);
            /// Schema::new((kind, vat));
            /// ```
            pub fn visible_when(
                mut self,
                watched: &impl Watched<F>,
                values: impl IntoIterator<Item = impl Into<String>>,
            ) -> Self {
                self.0.condition = Some(Condition::new(watched, values));
                self
            }
        }

        impl<F> From<$builder<F>> for Field {
            fn from(builder: $builder<F>) -> Field {
                builder.0
            }
        }

        impl<F> IntoSchema<F> for $builder<F> {
            fn into_schema(self) -> Schema<F> {
                self.0.into_schema().retype()
            }
        }

        impl<F> Retype for $builder<F> {
            type As<G> = $builder<G>;

            fn retype<G>(self) -> $builder<G> {
                $builder(self.0, PhantomData)
            }
        }
    };
}

/// The base modifiers, `help`, `disabled` and `custom`, on every builder of one value's control.
macro_rules! common_modifiers {
    ($builder:ident) => {
        base_modifiers!($builder);

        impl<F> $builder<F> {
            /// Renders `text` under the control, which the control names as its description.
            pub fn help(mut self, text: impl Into<String>) -> Self {
                self.0.help = Some(text.into());
                self
            }

            /// Renders the control disabled on the create and edit forms. The server drops a
            /// value posted for it anyway, so an edit keeps the stored value and a create takes
            /// the field's `default`, else its blank answer; a field with neither cannot be
            /// created, and its panel refuses to mount.
            pub fn disabled(mut self) -> Self {
                self.0.disabled = super::Disabled::Always;
                self
            }

            /// Renders the control disabled on the edit form only: the create form sets the
            /// value, and no edit changes it.
            pub fn disabled_on_edit(mut self) -> Self {
                self.0.disabled = super::Disabled::OnEdit;
                self
            }

            /// Renders the field with an app's [`Control`] instead, as [`Field::custom`] does:
            /// how a record form's control takes a custom input. The field keeps its key and
            /// label; the replaced control's own modifiers, such as `email`, `unique` or
            /// `options`, are dropped with it.
            pub fn custom(mut self, control: impl Control + 'static) -> CustomField<F> {
                self.0.control = ControlKind::Custom(Arc::new(control));
                self.0.checkbox = false;
                CustomField(self.0, PhantomData)
            }
        }
    };
}

/// Renders the control required, as a panel does for a record-form field with no blank answer.
#[cfg(test)]
macro_rules! required_for_tests {
    ($($builder:ident),*) => {$(
        impl<F> $builder<F> {
            pub(crate) fn required(mut self) -> Self {
                self.0.set_required(true);
                self
            }
        }
    )*};
}

#[cfg(test)]
required_for_tests!(TextField);

/// `default`, on every builder whose control a value fills.
macro_rules! default_modifier {
    ($($builder:ident),*) => {$(
        impl<F> $builder<F> {
            /// Fills the create form's control with `value`: any [`FormScalar`] or a `&str` in
            /// the form spelling, or, for a [`multiple`](ChoiceField::multiple) choice, an array
            /// or `Vec` of them. On a [`disabled`](Self::disabled) field it is the value a create
            /// stores. A schema refuses a default the control never posts: one a typed field
            /// does not parse, one not among a choice's options, a list on a control posting one
            /// value, and one value on a multiple choice.
            ///
            /// ```rust
            /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
            /// # struct Post { #[key] #[auto] id: uuid::Uuid, status: String, views: i64 }
            /// # use tablo_core::Field;
            /// Field::choice(Post::fields().status())
            ///     .options(["draft", "published"])
            ///     .default("draft");
            /// Field::text(Post::fields().views()).default(0_i64).disabled();
            /// Field::choice_input("channels")
            ///     .options(["email", "sms"])
            ///     .multiple()
            ///     .default(["email"]);
            /// ```
            pub fn default(mut self, value: impl IntoFormValue) -> Self {
                self.0.default = Some(value.into_form_value());
                self
            }
        }
    )*};
}

default_modifier!(TextField, ChoiceField, CustomField);

/// A value a field's `default` takes: any [`FormScalar`] or a `&str` or `&String` in the form
/// spelling, or an array or `Vec` of them for a multiple choice.
pub trait IntoFormValue {
    /// The value as its control posts it.
    fn into_form_value(self) -> FormValue;
}

impl<T: FormScalar> IntoFormValue for T {
    fn into_form_value(self) -> FormValue {
        FormValue::Text(self.to_form())
    }
}

impl IntoFormValue for &str {
    fn into_form_value(self) -> FormValue {
        FormValue::Text(self.to_string())
    }
}

impl IntoFormValue for &String {
    fn into_form_value(self) -> FormValue {
        FormValue::Text(self.clone())
    }
}

impl<T: FormScalar> IntoFormValue for Vec<T> {
    fn into_form_value(self) -> FormValue {
        FormValue::List(self.iter().map(FormScalar::to_form).collect())
    }
}

impl IntoFormValue for Vec<&str> {
    fn into_form_value(self) -> FormValue {
        FormValue::List(self.into_iter().map(str::to_string).collect())
    }
}

impl<T: FormScalar, const N: usize> IntoFormValue for [T; N] {
    fn into_form_value(self) -> FormValue {
        Vec::from(self).into_form_value()
    }
}

impl<const N: usize> IntoFormValue for [&str; N] {
    fn into_form_value(self) -> FormValue {
        Vec::from(self).into_form_value()
    }
}

/// A text field: [`Field::text`], or a record form's text control (`F` is the form).
pub struct TextField<F = ()>(pub(super) Field, PhantomData<fn() -> F>);

/// A choice field: [`Field::choice`], or a record form's choice control (`F` is the form).
pub struct ChoiceField<F = ()>(pub(super) Field, PhantomData<fn() -> F>);

/// A file field: [`Field::file`], or a record form's file control (`F` is the form).
pub struct FileField<F = ()>(pub(super) Field, PhantomData<fn() -> F>);

/// A repeater: [`Field::repeater`], or a record form's `#[form(repeat)]` control (`F` is the
/// form).
pub struct RepeaterField<F = ()>(pub(super) Field, PhantomData<fn() -> F>);

/// A field an app's [`Control`] renders: [`Field::custom`] and [`Field::toggle`], or a record
/// form's toggle (`F` is the form).
pub struct CustomField<F = ()>(pub(super) Field, PhantomData<fn() -> F>);

impl<F> CustomField<F> {
    /// Whether the control is the built-in checkbox, which a condition follows through `checked`.
    pub(crate) fn is_checkbox(&self) -> bool {
        self.0.is_checkbox()
    }
}

common_modifiers!(TextField);
common_modifiers!(ChoiceField);
common_modifiers!(FileField);
common_modifiers!(CustomField);
base_modifiers!(RepeaterField);

/// `choice`, on every builder but the choice's own.
macro_rules! to_choice {
    ($($builder:ident),*) => {$(
        impl<F> $builder<F> {
            /// Renders the field as a choice instead, as [`Field::choice`] does, ready for
            /// [`options`](ChoiceField::options) or [`relationship`](ChoiceField::relationship).
            /// The replaced control's own modifiers are dropped with it.
            pub fn choice(mut self) -> ChoiceField<F> {
                self.0.control = ControlKind::Choice(ChoiceControl::default());
                self.0.checkbox = false;
                ChoiceField(self.0, PhantomData)
            }
        }
    )*};
}

to_choice!(TextField, FileField, CustomField);

impl<F> TextField<F> {
    fn text(&mut self) -> &mut TextControl {
        match &mut self.0.control {
            ControlKind::Text(text) => text,
            _ => unreachable!("a TextField holds a text control"),
        }
    }

    /// Validate the value as an email address, and render `type="email"`.
    pub fn email(mut self) -> Self {
        self.text().email = true;
        self
    }

    /// Render `type="password"`: the browser masks the value, and a refused form renders the
    /// control empty rather than echoing the password back into the page. It renders the one-line
    /// input even when [`multiline`](Self::multiline).
    pub fn password(mut self) -> Self {
        self.text().password = true;
        self
    }

    /// A [`password`](Self::password) the user signs in with today, as a profile's current
    /// password: the control asks a password manager to fill the stored one.
    pub(crate) fn current_password(mut self) -> Self {
        let text = self.text();
        text.password = true;
        text.current_password = true;
        self
    }

    /// Probes a unique index before the write.
    pub fn unique(mut self) -> Self {
        self.text().unique = true;
        self
    }

    /// The control's placeholder text.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.text().placeholder = Some(placeholder.into());
        self
    }

    /// Render a `<textarea>` of `rows` lines rather than a one-line input,
    /// for a column holding prose.
    pub fn multiline(mut self, rows: u32) -> Self {
        self.text().rows = Some(rows);
        self
    }

    /// Refuse a value of fewer than `chars` characters, and render `minlength`.
    ///
    /// Like every rule here, the browser checks it before it submits, and the server checks it
    /// again on the trimmed value once the field's type parses it. A blank value is the field's
    /// blank answer, which no rule refuses, and a value the record already stores is not checked
    /// again. A length counts characters, where a browser counts UTF-16 units.
    pub fn min_length(mut self, chars: usize) -> Self {
        self.text().rules.min_length = Some(chars);
        self
    }

    /// Refuse a value of more than `chars` characters, and render `maxlength`, which stops the
    /// browser's input there.
    pub fn max_length(mut self, chars: usize) -> Self {
        self.text().rules.max_length = Some(chars);
        self
    }

    /// Refuse a value below `bound`, and render `min`.
    ///
    /// The bound is a value of the field's type, which orders: a number, or a `jiff` date, time,
    /// date and time, or timestamp. `.min(0)` on an `i64` field, `.min(date(2024, 1, 1))` on a
    /// `jiff::civil::Date` one; a bound the type does not parse or order fails the mount. A time's
    /// control edits it to the minute, so give a time bound in whole minutes.
    pub fn min(mut self, bound: impl std::fmt::Display) -> Self {
        self.text().rules.min = Some(bound.to_string());
        self
    }

    /// Refuse a value above `bound`, and render `max`; [`min`](Self::min) says which bounds a
    /// field takes.
    pub fn max(mut self, bound: impl std::fmt::Display) -> Self {
        self.text().rules.max = Some(bound.to_string());
        self
    }

    /// Refuse a value the regular expression `pattern` does not match whole, and render it as
    /// the `pattern` attribute.
    ///
    /// The server matches it with the `regex` crate and the browser as a JavaScript expression in
    /// its `v` mode, both against the whole value, so write it in the syntax they share: classes,
    /// groups, alternation and repetition. Spell digits `[0-9]`, since the server's `\d` matches
    /// other scripts' digits too; escape a `-` inside a class, which the browser otherwise refuses
    /// along with the whole pattern; and leave out lookaround, backreferences and inline flags.
    /// One the `regex` crate does not compile fails the mount. A `.multiline` textarea renders no
    /// `pattern`, so only the server checks it there.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Product { #[key] #[auto] id: uuid::Uuid, sku: String }
    /// # use tablo_core::{Field, lens};
    /// Field::text(lens!(Product.sku)).pattern("[A-Z]{3}-[0-9]{4}");
    /// ```
    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.text().rules.pattern = Some(Pattern::new(pattern.into()));
        self
    }
}

impl<F> ChoiceField<F> {
    fn choice_mut(&mut self) -> &mut ChoiceControl {
        match &mut self.0.control {
            ControlKind::Choice(choice) => choice,
            _ => unreachable!("a ChoiceField holds a choice control"),
        }
    }

    /// Static options: values that are their own label
    /// (`["draft", "published"]`), `(value, label)` pairs, or a
    /// [`#[derive(Options)]`](crate::Options) enum's
    /// [`options()`](crate::schema::Options::options).
    pub fn options(mut self, options: impl IntoOptions) -> Self {
        self.choice_mut().options = options.into_options();
        self
    }

    /// Takes several options: a checkbox per option, posting the field's key once per checked
    /// box. A record form's many-to-many field (`Vec<K>` with `#[form(relationship = R)]`) is one;
    /// [`parse_list`](crate::values::parse_list) reads one in an action's input.
    ///
    /// It offers at most [`MAX_RELATIONSHIP_OPTIONS`](crate::schema::MAX_RELATIONSHIP_OPTIONS)
    /// records: past them it lists none but the chosen ones, and refuses a submission choosing
    /// another. A record form's edit re-checks only the keys the record does not link yet.
    /// `searchable` filters the listed options as the user types. A condition cannot watch it,
    /// it cannot depend on another field, and its `default` is a list.
    pub fn multiple(mut self) -> Self {
        self.choice_mut().multiple = true;
        self
    }

    /// Filters options as the user types, fetching from the relationship past the option cap.
    pub fn searchable(mut self) -> Self {
        self.choice_mut().searchable = true;
        self
    }

    /// Loads options from a related source's tenant-scoped query, each valued by its primary key
    /// and labelled by the source's [`label`](OptionSource::label), degrading to type-to-search
    /// past the option cap.
    ///
    /// A resource's options carry its [`record_title`](crate::ResourceDef::record_title), the
    /// title its detail page shows. [`relationship_labelled`](Self::relationship_labelled)
    /// labels one field's options otherwise.
    pub fn relationship<R>(mut self) -> Self
    where
        R: OptionSource + 'static,
    {
        self.choice_mut().relationship = Some(choice::Relationship::new::<R>(R::label));
        self
    }

    /// Offers only the related rows whose column `column` equals the value `parent` posts: the
    /// cities of the chosen country. The browser fetches the options again when `parent` changes,
    /// and a submission must name a row of the posted parent value. A blank parent offers none.
    ///
    /// `column` belongs to the [`relationship`](Self::relationship)'s source model; mounting
    /// refuses another model, a choice without a relationship, and a `parent` the schema does not
    /// place.
    pub fn depends_on<M, T>(
        mut self,
        parent: &impl Watched<F>,
        column: impl Into<toasty::stmt::Path<M, T>>,
    ) -> Self
    where
        M: Send + Sync + 'static,
        T: FormScalar + toasty::stmt::IntoExpr<T> + Send + Sync + 'static,
    {
        let column: toasty::stmt::Path<M, T> = column.into();
        self.choice_mut().parent = Some(choice::Parent {
            watched: parent.key().to_string(),
            model: std::any::TypeId::of::<M>(),
            scope: Arc::new(move |value: &str| {
                T::parse_form(value)
                    .ok()
                    .map(|value| column.clone().eq(value))
            }),
        });
        self
    }

    /// Loads options as [`relationship`](Self::relationship) does, labelling each by `label`
    /// instead of the source's label: to tell apart records that share a title. The option's
    /// record loads without its relations.
    pub fn relationship_labelled<R>(
        mut self,
        label: impl Fn(&R::Model) -> String + Send + Sync + 'static,
    ) -> Self
    where
        R: OptionSource + 'static,
    {
        self.choice_mut().relationship = Some(choice::Relationship::new::<R>(
            move |_cx: &topcoat::context::Cx, record: &R::Model| label(record),
        ));
        self
    }
}

impl<F> RepeaterField<F> {
    /// The add button's label, over "Add item".
    pub fn add_label(mut self, label: impl Into<String>) -> Self {
        match &mut self.0.control {
            ControlKind::Repeater(repeater) => repeater.add_label = Some(label.into()),
            _ => unreachable!("a RepeaterField holds a repeater control"),
        }
        self
    }
}

/// A choice's static options as `(value, label)` pairs, in display order.
pub trait IntoOptions {
    fn into_options(self) -> Vec<(String, String)>;
}

impl IntoOptions for Vec<String> {
    fn into_options(self) -> Vec<(String, String)> {
        self.into_iter()
            .map(|value| (value.clone(), value))
            .collect()
    }
}

impl IntoOptions for Vec<(String, String)> {
    fn into_options(self) -> Vec<(String, String)> {
        self
    }
}

impl<const N: usize> IntoOptions for [&str; N] {
    fn into_options(self) -> Vec<(String, String)> {
        self.into_iter()
            .map(|value| (value.to_string(), value.to_string()))
            .collect()
    }
}

/// The crate's tests read a builder's field as the schema will hold it.
#[cfg(test)]
mod test_deref {
    use super::{ChoiceField, CustomField, Field, FileField, RepeaterField, TextField};

    macro_rules! deref_field {
        ($($builder:ident),*) => {$(
            impl<F> std::ops::Deref for $builder<F> {
                type Target = Field;
                fn deref(&self) -> &Field {
                    &self.0
                }
            }
        )*};
    }

    deref_field!(
        TextField,
        ChoiceField,
        FileField,
        CustomField,
        RepeaterField
    );
}
