//! The typed field builders the [`Field`] constructors return.
//!
//! Each builder wraps a `Field` whose control is its own kind and offers only
//! that control's modifiers, so `.placeholder(..)` on a choice is a compile
//! error rather than a declaration the panel refuses.

use topcoat::context::Cx;

use super::{
    super::{IntoSchema, OptionSource, Schema, relationship::RelatedPrimaryKey},
    ChoiceControl, ControlKind, Field, TextControl, choice,
};

/// `label`, `required` and `optional`, shared by every builder.
macro_rules! common_modifiers {
    ($builder:ident) => {
        impl $builder {
            /// Override the label.
            pub fn label(mut self, label: impl Into<String>) -> Self {
                self.0.label = label.into();
                self
            }

            /// Refuse an empty submission.
            pub fn required(mut self) -> Self {
                self.0.required = true;
                self
            }

            /// Accept an empty submission: for a nullable column, or for a
            /// column a record fn fills when the form leaves it empty. The
            /// browser-side `required` attribute drops too.
            pub fn optional(mut self) -> Self {
                self.0.required = false;
                self
            }

            /// The key the control posts.
            pub fn name(&self) -> &str {
                &self.0.name
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
        self.0.rules.set_email();
        self
    }

    /// Mark the field as backed by a unique index, which the app-side
    /// pre-check probes before the write.
    ///
    /// **Uniqueness implies presence on a non-nullable column**: an empty
    /// `String` stores `""`, which the index admits only once, so an empty
    /// submit is refused inline as `"<Label> is required"` instead of being
    /// written, and the probe never sees it. `.optional()` does not lift that
    /// rule, whichever order the two are called in (ADR-0010). A nullable
    /// column (`Option<T>`) stores NULL for an empty submit, which the index
    /// admits any number of times, so it stays optional when declared so.
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
    /// (`vec!["draft".into()]`), `(value, label)` pairs, or a
    /// [`#[derive(Options)]`](crate::Options) enum's
    /// [`options()`](crate::schema::Options::options).
    pub fn options(mut self, options: impl IntoOptions) -> Self {
        self.choice().options = options.into_options();
        self
    }

    /// Filter the options as the user types.
    ///
    /// Renders a filter input and a suggestion listbox above the select.
    /// Typing narrows the list by label substring for a bounded set, and a pick
    /// writes the chosen option onto the select, which stays the form control.
    /// Past the option cap, a relationship fetches
    /// `GET {parent_list_url}/options?field=&q=` (debounced, in-flight
    /// requests aborted, selection preserved) and re-renders the list from the
    /// answer, searching the related table's `searchable()` columns; a
    /// non-searchable relationship keeps the cap error.
    ///
    /// Needs `assets/selects.js` (`tablo_ui::SELECTS_JS`), emitted by
    /// `Panel::render_document` on every document with shell assets
    /// (ADR-0014); without it the input is inert and the plain select keeps
    /// working.
    pub fn searchable(mut self) -> Self {
        self.choice().searchable = true;
        self
    }

    /// Load the options from a related source's tenant-scoped query.
    ///
    /// `R` is any [`OptionSource`] — every `Resource` is one. The first
    /// argument is the related resource's `query` fn (`AuthorResource::query`),
    /// a type-inference witness only: the loader calls the source's scoped
    /// query, so the tenant gate and the derived tenant filter apply. The
    /// second projects a record to the model's **primary key**, whose
    /// `Display` is the `<option value>`; the third maps it to its label. A
    /// projection to anything but the key type fails to compile, and an edit
    /// form hydrates the foreign key with the same string.
    ///
    /// Policy-checked: the related source must allow `can_view_any` and, when
    /// it declares `requires_tenant`, have a resolved tenant; each loaded row
    /// is filtered through `can_view`. A denial fails the load closed — no
    /// options and not the stored value, `{label} is not available` on GET,
    /// and a submit that carries a value fails with that message.
    ///
    /// Bounded and memoized: at most one row past `MAX_RELATIONSHIP_OPTIONS`
    /// loads per `(request, tenant)`. Past the cap a searchable field degrades
    /// to type-to-search with a targeted existence check, and a non-searchable
    /// one reports `could not load options, retry`.
    pub fn relationship<R>(
        mut self,
        query: fn(&Cx) -> toasty::stmt::Query<toasty::stmt::List<R::Model>>,
        value: impl Fn(&R::Model) -> RelatedPrimaryKey<R> + Send + Sync + 'static,
        label: impl Fn(&R::Model) -> String + Send + Sync + 'static,
    ) -> Self
    where
        R: OptionSource + 'static,
        RelatedPrimaryKey<R>: std::fmt::Display,
    {
        let _ = query;
        self.choice().relationship = Some(choice::Relationship::new::<R>(value, label));
        self
    }
}

/// A choice's static options as `(value, label)` pairs, in display order.
///
/// Implemented for a list of values that are their own label, for a list of
/// pairs, and for an array of string slices.
pub trait IntoOptions {
    /// The `(value, label)` pairs.
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
