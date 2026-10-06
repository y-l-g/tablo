//! The typed builders the [`Field`] constructors return, each offering only its control's modifiers
//! so a modifier on the wrong control does not compile.

use super::{
    super::{IntoSchema, OptionSource, Schema},
    ChoiceControl, ControlKind, Field, TextControl, choice,
};

/// `label` and `name`, shared by every builder.
macro_rules! common_modifiers {
    ($builder:ident) => {
        impl $builder {
            pub fn label(mut self, label: impl Into<String>) -> Self {
                self.0.label = Some(label.into());
                self
            }

            pub fn name(&self) -> &str {
                self.0.name()
            }
        }

        impl From<$builder> for Field {
            fn from(builder: $builder) -> Field {
                builder.0
            }
        }

        impl IntoSchema for $builder {
            fn into_schema(self) -> Schema {
                self.0.into_schema()
            }
        }
    };
}

/// Renders the control required, as a panel does for a record-form field with no blank answer.
#[cfg(test)]
macro_rules! required_for_tests {
    ($($builder:ident),*) => {$(
        impl $builder {
            pub(crate) fn required(mut self) -> Self {
                self.0.set_required(true);
                self
            }
        }
    )*};
}

#[cfg(test)]
required_for_tests!(TextField, FileField);

/// A text field: [`Field::text`].
pub struct TextField(pub(super) Field);

/// A choice field: [`Field::choice`].
pub struct ChoiceField(pub(super) Field);

/// A file field: [`Field::file`].
pub struct FileField(pub(super) Field);

/// A field an app's [`Control`](super::Control) renders: [`Field::custom`]
/// and [`Field::toggle`].
pub struct CustomField(pub(super) Field);

common_modifiers!(TextField);
common_modifiers!(ChoiceField);
common_modifiers!(FileField);
common_modifiers!(CustomField);

impl TextField {
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

impl ChoiceField {
    fn choice(&mut self) -> &mut ChoiceControl {
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
        self.choice().options = options.into_options();
        self
    }

    /// Filters options as the user types, fetching from the relationship past the option cap.
    pub fn searchable(mut self) -> Self {
        self.choice().searchable = true;
        self
    }

    /// Loads options from a related source's tenant-scoped query, each valued by its primary key
    /// and labelled by `label`, degrading to type-to-search past the option cap.
    pub fn relationship<R>(
        mut self,
        label: impl Fn(&R::Model) -> String + Send + Sync + 'static,
    ) -> Self
    where
        R: OptionSource + 'static,
    {
        self.choice().relationship = Some(choice::Relationship::new::<R>(label));
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
    use super::{ChoiceField, CustomField, Field, FileField, TextField};

    macro_rules! deref_field {
        ($($builder:ident),*) => {$(
            impl std::ops::Deref for $builder {
                type Target = Field;
                fn deref(&self) -> &Field {
                    &self.0
                }
            }
        )*};
    }

    deref_field!(TextField, ChoiceField, FileField, CustomField);
}
