//! The typed builders the [`Field`] constructors return, each offering only its control's modifiers
//! so a modifier on the wrong control does not compile.

use std::{marker::PhantomData, sync::Arc};

use super::{
    super::{
        IntoSchema, OptionSource, Retype, Schema,
        condition::{Condition, Watched},
    },
    ChoiceControl, Control, ControlKind, Field, TextControl, choice,
};
use crate::form::FormScalar;

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
            /// cannot be conditional.
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

/// The base modifiers and `custom`, on every builder of one value's control.
macro_rules! common_modifiers {
    ($builder:ident) => {
        base_modifiers!($builder);

        impl<F> $builder<F> {
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
required_for_tests!(TextField, FileField);

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
            /// Unlike `#[form(relationship = ..)]`, it leaves the record form's derived table and
            /// detail page as they are. The replaced control's own modifiers are dropped with it.
            pub fn choice(mut self) -> ChoiceField<F> {
                self.0.control = ControlKind::Choice(ChoiceControl::default());
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
