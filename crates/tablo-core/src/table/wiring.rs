//! A declared [`Table`] together with what one request wires onto it.

use std::{ops::Deref, sync::Arc};

use super::{RowActions, RowPolicy, Table, TableAction, with_return};

/// What a request wires onto a declared table: the action URLs its rows and bulk bar link to,
/// and which actions each row allows.
///
/// The panel builds it per request from the mounted resource and the request's policy, so the
/// declaration itself carries none of it.
pub(crate) struct Wiring<M> {
    row_policy: Option<RowPolicy<M>>,
    delete_prefix: Option<String>,
    edit_prefix: Option<String>,
    view_prefix: Option<String>,
    bulk_delete: bool,
    /// The custom actions and the list URL their routes hang off.
    custom_actions: Vec<TableAction<M>>,
    actions_prefix: Option<String>,
    /// Where a write this table's row and bulk actions start lands.
    return_to: Option<String>,
}

impl<M> Default for Wiring<M> {
    fn default() -> Self {
        Self {
            row_policy: None,
            delete_prefix: None,
            edit_prefix: None,
            view_prefix: None,
            bulk_delete: false,
            custom_actions: Vec::new(),
            actions_prefix: None,
            return_to: None,
        }
    }
}

/// A declared [`Table`] with one request's actions and policy wired on: what the list page
/// renders.
///
/// [`wired_table`](crate::panel::wired_table) returns the panel's for a resource. It reads as
/// its [`Table`] for loading a [`TablePage`](crate::TablePage).
pub struct WiredTable<M> {
    table: Arc<Table<M>>,
    pub(super) wiring: Wiring<M>,
}

impl<M> Deref for WiredTable<M> {
    type Target = Table<M>;

    fn deref(&self) -> &Table<M> {
        &self.table
    }
}

impl<M> std::fmt::Debug for WiredTable<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let wiring = &self.wiring;
        f.debug_struct("WiredTable")
            .field("table", &self.table)
            .field("row_policy", &wiring.row_policy.is_some())
            .field("delete_prefix", &wiring.delete_prefix)
            .field("edit_prefix", &wiring.edit_prefix)
            .field("view_prefix", &wiring.view_prefix)
            .field("bulk_delete", &wiring.bulk_delete)
            .field(
                "custom_actions",
                &wiring
                    .custom_actions
                    .iter()
                    .map(|a| a.name)
                    .collect::<Vec<_>>(),
            )
            .field("return_to", &wiring.return_to)
            .finish()
    }
}

impl<M> WiredTable<M> {
    /// `table` with nothing wired: no actions.
    pub(crate) fn new(table: Arc<Table<M>>) -> Self {
        Self {
            table,
            wiring: Wiring::default(),
        }
    }

    /// Declare the per-record action policy.
    pub(crate) fn row_actions(
        mut self,
        policy: impl Fn(&M) -> RowActions + Send + Sync + 'static,
    ) -> Self {
        self.wiring.row_policy = Some(Arc::new(policy));
        self
    }

    /// Enable the row-level `Delete` action.
    pub(crate) fn with_delete(mut self, prefix: String) -> Self {
        if self.addressable {
            self.wiring.delete_prefix = Some(prefix);
        }
        self
    }

    /// Enable the row-level `Edit` action.
    pub(crate) fn with_edit(mut self, prefix: String) -> Self {
        if self.addressable {
            self.wiring.edit_prefix = Some(prefix);
        }
        self
    }

    /// Enable the row-level `View` action.
    pub(crate) fn with_view(mut self, prefix: String) -> Self {
        if self.addressable {
            self.wiring.view_prefix = Some(prefix);
        }
        self
    }

    /// Enable bulk selection with the `BulkDelete` action.
    pub(crate) fn with_bulk_delete(mut self, enabled: bool) -> Self {
        self.wiring.bulk_delete = enabled;
        self
    }

    /// Wire the resource's custom actions.
    pub(crate) fn with_custom_actions(
        mut self,
        prefix: String,
        actions: Vec<TableAction<M>>,
    ) -> Self {
        if self.addressable {
            self.wiring.actions_prefix = Some(prefix);
            self.wiring.custom_actions = actions;
        }
        self
    }

    /// Send the writes this table starts back to `url`.
    pub(crate) fn returning_to(mut self, url: String) -> Self {
        self.wiring.return_to = Some(url);
        self
    }

    /// Which row actions `record` allows.
    pub(crate) fn actions_for(&self, record: &M) -> RowActions {
        self.wiring
            .row_policy
            .as_ref()
            .map_or(RowActions::ALL, |policy| policy(record))
    }

    pub(super) fn action_url(&self, url: String) -> String {
        match &self.wiring.return_to {
            Some(target) => with_return(&url, target),
            None => url,
        }
    }

    pub(super) fn delete_prefix(&self) -> Option<&str> {
        self.wiring.delete_prefix.as_deref()
    }

    pub(super) fn edit_prefix(&self) -> Option<&str> {
        self.wiring.edit_prefix.as_deref()
    }

    pub(super) fn view_prefix(&self) -> Option<&str> {
        self.wiring.view_prefix.as_deref()
    }

    pub(super) fn actions_prefix(&self) -> Option<&str> {
        self.wiring.actions_prefix.as_deref()
    }

    pub(super) fn row_custom_actions(&self) -> impl Iterator<Item = &TableAction<M>> {
        self.wiring
            .custom_actions
            .iter()
            .filter(|a| a.row && self.wiring.actions_prefix.is_some())
    }

    pub(super) fn bulk_custom_actions(&self) -> impl Iterator<Item = &TableAction<M>> {
        self.wiring
            .custom_actions
            .iter()
            .filter(|a| a.bulk && self.wiring.actions_prefix.is_some())
    }

    pub(super) fn bulk_delete_enabled(&self) -> bool {
        self.wiring.bulk_delete && self.wiring.delete_prefix.is_some()
    }

    pub(super) fn bulk_enabled(&self) -> bool {
        self.bulk_delete_enabled() || self.bulk_custom_actions().next().is_some()
    }
}

#[cfg(test)]
impl<M> Table<M> {
    /// This table with nothing wired yet, for a test that wires it by hand.
    pub(crate) fn wired(self) -> WiredTable<M> {
        WiredTable::new(Arc::new(self))
    }
}
