//! The modifiers saying when a control renders disabled or not at all, beyond `disabled` and
//! `disabled_on_edit`: on one of a record form's two forms, or while another field posts a value.

use super::{ChoiceField, CustomField, FileField, TextField};
use crate::schema::condition::{Condition, Watched};
#[cfg(feature = "rich-text")]
use crate::schema::fields::RichTextField;

/// `hidden_on_create` and `hidden_on_edit`, on the builders of a record form's controls.
macro_rules! form_visibility {
    ($builder:ident) => {
        impl<F: crate::schema::EditsRecords> $builder<F> {
            /// Renders nothing for the field on the create form. The create stores the field's
            /// `default`, else its blank answer: a required field with neither cannot mount. A
            /// field hidden on both forms only ever stores that: set it in the record instead.
            pub fn hidden_on_create(mut self) -> Self {
                self.0.hidden.create = true;
                self
            }

            /// Renders nothing for the field on the edit form, and keeps its stored value whatever
            /// an edit posts: the create form sets the value, and no edit changes it, as
            /// [`disabled_on_edit`](Self::disabled_on_edit) does without showing it.
            pub fn hidden_on_edit(mut self) -> Self {
                self.0.hidden.edit = true;
                self
            }
        }
    };
}

/// `disabled_when`, on the builders whose control a disabled fieldset disables: a rich text
/// editor reads its state once, as it starts.
macro_rules! conditional_disable {
    ($builder:ident) => {
        impl<F> $builder<F> {
            /// Renders the control disabled while `watched` posts one of `values`: an option's
            /// value, or `true` or `false` for a toggle. Disabled, the control posts nothing, so
            /// an edit keeps the stored value and a create takes the field's `default`, else its
            /// blank answer.
            ///
            /// ```rust
            /// # #[derive(Debug, Clone, toasty::Model)]
            /// # struct Order {
            /// #     #[key] #[auto] id: uuid::Uuid,
            /// #     status: String,
            /// #     total: i64,
            /// # }
            /// # use tablo_core::{Field, Schema};
            /// let status = Field::choice(Order::fields().status()).options(["draft", "sent"]);
            /// let total = Field::text(Order::fields().total()).disabled_when(&status, ["sent"]);
            /// Schema::new((status, total));
            /// ```
            ///
            /// - The browser enables and disables the control as the watched field changes, with no
            ///   request, through a disabled fieldset around it. An app's own
            ///   [`Control`](crate::extend::Control) follows it only through the form controls it
            ///   renders. Without JavaScript, the control starts as the form's initial values set
            ///   it, the stored ones on an edit, and stays that way.
            /// - The panel reads a submission the same way and drops the field's posted key, and a
            ///   file field's carried upload, while the condition holds. The condition reads what
            ///   the user may post, so it is no access check:
            ///   [`ResourceDef::field_access`](crate::ResourceDef::field_access) is.
            /// - Mounting refuses what it refuses for [`visible_when`](Self::visible_when): an
            ///   unplaced watched field, a value it never posts, a multiple choice, and a watched
            ///   field another condition hides while this one still shows. It also refuses a
            ///   required field with no non-blank `default`, which a create could not fill while
            ///   the condition holds, and a condition or a dependent choice watching this field: a
            ///   disabled control posts nothing, so the browser and the server would read different
            ///   values.
            /// - A later call replaces an earlier one.
            pub fn disabled_when(
                mut self,
                watched: &impl Watched<F>,
                values: impl IntoIterator<Item = impl Into<String>>,
            ) -> Self {
                self.0.disabling = Some(Condition::new(watched, values));
                self
            }
        }
    };
}

form_visibility!(TextField);
form_visibility!(ChoiceField);
form_visibility!(FileField);
form_visibility!(CustomField);
#[cfg(feature = "rich-text")]
form_visibility!(RichTextField);
conditional_disable!(TextField);
conditional_disable!(ChoiceField);
conditional_disable!(FileField);
conditional_disable!(CustomField);
