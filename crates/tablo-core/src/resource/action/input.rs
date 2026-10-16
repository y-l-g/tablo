//! What an [`Action`](super::Action)'s input form refuses or marks: its reserved keys, its required
//! controls.

use crate::schema::Field;

/// Renders `field`'s control required: the derive marks each field with no blank answer.
#[doc(hidden)]
pub fn required_input(field: impl Into<Field>) -> Field {
    let mut field = field.into();
    field.set_required(true);
    field
}

/// The keys an action's POST carries besides its input, which no input field may post.
pub(crate) const RESERVED_KEYS: [&str; 3] = [crate::csrf::FIELD_NAME, "confirm", "ids"];
