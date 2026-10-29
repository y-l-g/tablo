//! When a field's control renders disabled or not at all, so the browser posts nothing for it.

use super::{Field, Placement};
use crate::schema::condition::Condition;

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

/// The forms a [`Field`] renders nothing on, which the server reads as it reads a disabled
/// control.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Hidden {
    pub(crate) create: bool,
    pub(crate) edit: bool,
}

impl Field {
    /// Whether the control renders disabled, or nothing, on the edit form when `editing`, else
    /// the create form: either way the browser posts nothing for it.
    pub(crate) fn is_disabled(&self, editing: bool) -> bool {
        self.is_hidden(editing)
            || match self.disabled {
                Disabled::Never => false,
                Disabled::Always => true,
                Disabled::OnEdit => editing,
            }
    }

    /// Whether the form renders nothing for the field, on the edit form when `editing`, else the
    /// create form.
    pub(crate) fn is_hidden(&self, editing: bool) -> bool {
        if editing {
            self.hidden.edit
        } else {
            self.hidden.create
        }
    }

    /// The condition disabling the control, if any.
    pub(crate) fn disabling(&self) -> Option<&Condition> {
        self.disabling.as_ref()
    }

    /// Whether the control renders disabled where `placement` puts it.
    pub(crate) fn is_disabled_at(&self, placement: &Placement<'_>) -> bool {
        placement.locked || self.is_disabled(placement.editing)
    }

    /// Whether the create form posts nothing the control could set: it renders disabled or
    /// nothing there, or a condition can disable it.
    pub(crate) fn may_post_nothing_on_create(&self) -> bool {
        self.is_disabled(false) || self.disabling.is_some()
    }
}
