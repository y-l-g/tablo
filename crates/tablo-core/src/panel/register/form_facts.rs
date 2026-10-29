//! What a `has_one` relation's related form needs of the resources it joins, checked at mount.

use crate::{
    RelatedFormFault,
    form::RecordForm,
    resource::{Mounted, Resource},
};

/// What a `has_one` relation's related form needs of a registered resource.
pub(in crate::panel) struct FormFacts {
    /// Whether the resource has a record form.
    has_form: bool,
    /// Every key its form posts.
    keys: Vec<String>,
    soft_deletes: bool,
    /// The first field of its form a related form cannot carry.
    own_keys: Option<String>,
}

impl FormFacts {
    pub(super) fn of<R: Resource>(mounted: &Mounted<R>) -> Self {
        Self {
            has_form: <R::Form as RecordForm>::HAS_FORM,
            keys: mounted
                .form
                .fields()
                .map(|field| field.name().to_string())
                .collect(),
            soft_deletes: mounted.soft_delete.is_some(),
            own_keys: mounted.form.needs_own_keys().map(str::to_string),
        }
    }

    /// Why `child`'s form cannot render inside this owner's, its controls posting under
    /// `{slug}.` and `foreign_key` filled by the owner.
    pub(super) fn related_faults(
        &self,
        child: &Self,
        slug: &str,
        foreign_key: &str,
    ) -> Vec<RelatedFormFault> {
        let mut faults = Vec::new();
        if !self.has_form {
            faults.push(RelatedFormFault::NoOwnerForm);
        }
        if !child.has_form {
            faults.push(RelatedFormFault::NoForm);
        } else if !child.keys.iter().any(|key| key == foreign_key) {
            faults.push(RelatedFormFault::UnboundForeignKey {
                column: foreign_key.to_string(),
            });
        }
        if child.soft_deletes {
            faults.push(RelatedFormFault::SoftDeletes);
        }
        if let Some(field) = &child.own_keys {
            faults.push(RelatedFormFault::OwnKeys {
                field: field.clone(),
            });
        }
        // A key equal to the slug shadows the prefix too: a repeater named so posts its rows
        // under `{slug}.`.
        let prefix = format!("{slug}.");
        faults.extend(
            self.keys
                .iter()
                .filter(|key| *key == slug || key.starts_with(&prefix))
                .map(|key| RelatedFormFault::ShadowedKey { field: key.clone() }),
        );
        faults
    }
}
