//! A table's groupings: the fields a reader may group its rows by. A grouping orders the query
//! by its field ahead of the sort, so a group runs on across pages rather than within each.

use std::sync::Arc;

use derive_where::derive_where;
use toasty::stmt::OrderByExpr;

use super::{Table, TableState};
use crate::{DeclarationErrorKind, Lens, schema::Binding, values::FormScalar};

/// Group-label projection reads a row's group off one model instance: `None` for a row holding
/// no value.
pub(crate) type GroupKey<M> = Arc<dyn Fn(&M) -> Option<crate::Text> + Send + Sync>;

/// A named grouping a `Table` offers.
#[derive_where(Clone)]
pub(crate) struct GroupDef<M> {
    binding: Binding,
    key: GroupKey<M>,
    /// The field's ascending order-by, which the grouping puts ahead of the sort.
    order: OrderByExpr,
}

impl<M> GroupDef<M> {
    /// The `?group_by=` value naming this grouping: its field's name.
    pub(crate) fn name(&self) -> &str {
        self.binding.name()
    }

    /// The grouping's label, its field's.
    pub(crate) fn label(&self) -> crate::Text {
        self.binding.label()
    }

    /// The label of the group `record` falls in.
    pub(crate) fn key(&self) -> &GroupKey<M> {
        &self.key
    }

    /// The collapse token of the group labelled `label`, `None` for the rows holding no value:
    /// the grouping's name, so a group collapsed under one grouping leaves another's be, then
    /// the label, as [`bulk_token`](super::bulk_token) spells it. No token is empty, so none
    /// matches inside another's commas.
    pub(crate) fn token(&self, label: Option<&str>) -> String {
        match label {
            Some(label) => super::bulk_token(&format!("{}={label}", self.name())),
            None => super::bulk_token(&format!("{}!", self.name())),
        }
    }

    /// What the grouping reads of a record.
    pub(super) fn reads(&self) -> crate::lens::Reads<M> {
        self.binding.reads::<M>()
    }

    /// The ascending order-by putting each group's rows together.
    pub(super) fn order(&self) -> &OrderByExpr {
        &self.order
    }
}

impl<M> Table<M> {
    /// Offer grouping the rows by the field `lens` reads, under the field's name. Call it once
    /// per field a reader may group by; `?group_by=` picks one, and the list groups by none
    /// without it.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String, status: String, author: String }
    /// use tablo_core::{Table, TextColumn, lens};
    ///
    /// Table::new(TextColumn::new(lens!(Post.title)))
    ///     .group_by(lens!(Post.status))
    ///     .group_by(lens!(Post.author));
    /// ```
    ///
    /// A grouping orders the rows by its field ahead of the sort, so each group runs on across
    /// pages, and its header shows its field's value, or "No value" over the rows holding none.
    /// Two groupings by one field are a declaration error.
    pub fn group_by<T>(mut self, lens: Lens<M, T>) -> Self
    where
        M: toasty::schema::Model + Send + Sync + 'static,
        T: FormScalar + Send + Sync + 'static,
    {
        let order = lens.path().clone().asc();
        self.groupings.push(GroupDef {
            binding: Binding::of(lens.path()),
            key: Arc::new(move |record| {
                let value = lens.read(record);
                (!value.to_form().is_empty()).then(|| value.to_label())
            }),
            order,
        });
        self
    }

    /// The groupings a reader may pick, in declaration order.
    pub(crate) fn declared_groupings(&self) -> &[GroupDef<M>] {
        &self.groupings
    }

    /// The grouping `state.group_by` names, if the table declares it.
    pub(crate) fn active_grouping(&self, state: &TableState) -> Option<&GroupDef<M>> {
        let want = state.group_by.as_deref()?;
        self.groupings.iter().find(|def| def.name() == want)
    }

    /// Normalize `state.group_by` against the declared groupings.
    pub(crate) fn normalize_state(&self, state: &TableState) -> TableState {
        let mut out = state.clone();
        if self.active_grouping(state).is_none() {
            out.group_by = None;
        }
        out
    }

    /// What is wrong with the groupings: a path naming no field, or two on one field.
    pub(super) fn grouping_errors(&self) -> Vec<DeclarationErrorKind> {
        let mut errors = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for def in &self.groupings {
            if let Some(error) = def.binding.misdeclared() {
                errors.push(error);
            } else if !seen.insert(def.name()) {
                errors.push(DeclarationErrorKind::DuplicateGrouping {
                    name: def.name().to_string(),
                });
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests;
