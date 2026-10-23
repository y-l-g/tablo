//! Unified Schema primitive — fields and layout blocks that compose via `view!`.
//!
//! `Schema` holds layout blocks, embedded values, and [`Field`] slots and resolves every field once
//! into one list for rendering and validation.

mod comparison;
mod condition;
pub(crate) mod embedded;
pub(crate) mod fields;
pub(crate) mod input;
mod layouts;
mod lenses;
mod mask;
mod options;
mod read;
pub(crate) mod relationship;
pub(crate) mod repeater;
pub(crate) mod tree;

use std::{collections::HashSet, marker::PhantomData};

pub use condition::Watched;
pub use embedded::EmbeddedForm;
use fields::ChoiceControl;
pub use fields::{
    ChoiceField, CustomField, Field, FileField, IntoFormValue, IntoOptions, RepeaterField,
    RichTextField, Switch, TextField, Toggle,
};
pub(crate) use fields::{
    MAX_FORM_BYTES,
    file::{FileHref, file_href},
    is_linkable, is_rooted, option_view, read_only, stored_upload, value_cell,
};
pub(crate) use input::FormInput;
pub use layouts::{Grid, Group, HoldsTabs, Section, Tab, Tabs};
pub(crate) use layouts::{tab_id, tab_view, tabs_chrome};
pub(crate) use lenses::Binding;
pub use lenses::form_key;
pub(crate) use mask::FormMask;
pub use options::Options;
pub use relationship::MAX_RELATIONSHIP_OPTIONS;
pub(crate) use relationship::{
    OptionLoadError, OptionSource, clamp_query_term, ensure_option_access,
};
pub use repeater::{MAX_REPEATER_ROWS, parse_items, write_items};
use topcoat::{Result, context::Cx, view::*};
pub use tree::{IntoSchema, Source};
pub(crate) use tree::{Node, Retype, render_nodes};

use crate::values::{FieldErrors, FormValue, FormValues};

/// Composes fields and layout blocks and resolves every field once into one list.
///
/// `F` is the record form whose controls the schema places. A resource's form is a
/// `Schema<UserForm>`, which only `UserForm::controls()` fill, so a control from another form or a
/// field built with [`Field::text`] fails to compile there. A page's or an action's schema is a
/// plain `Schema`, which [`Field`]'s constructors fill.
pub struct Schema<F = ()> {
    pub(crate) nodes: Vec<Node>,
    pub(crate) fields: Vec<Field>,
    form: PhantomData<fn() -> F>,
}

impl<F> Default for Schema<F> {
    fn default() -> Self {
        Self::from_parts(Vec::new(), Vec::new())
    }
}

impl<F> std::fmt::Debug for Schema<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Schema")
            .field("nodes", &self.nodes)
            .field("fields", &self.fields)
            .finish()
    }
}

impl Schema {
    /// A schema with no field: a page's or an action's that renders nothing. A resource's form
    /// that places nothing is `Schema::default()`, which renders every control.
    pub fn empty() -> Self {
        Self::default()
    }
}

impl<F> Retype for Schema<F> {
    type As<G> = Schema<G>;

    fn retype<G>(self) -> Schema<G> {
        Schema::from_parts(self.nodes, self.fields)
    }
}

impl<F> Schema<F> {
    pub(crate) fn from_parts(nodes: Vec<Node>, fields: Vec<Field>) -> Self {
        Self {
            nodes,
            fields,
            form: PhantomData,
        }
    }

    /// Reports whether this schema declares nothing to render.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Builds a `Schema` from any `IntoSchema` and reports duplicate field names as declaration
    /// errors.
    pub fn new(children: impl IntoSchema<F>) -> Self {
        children.into_schema()
    }

    /// Every field, in the order it joins the schema: a layout's fields as it composes, in
    /// declaration order.
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter()
    }

    /// Renders each control required exactly when `required` names its key.
    pub(crate) fn require(&mut self, required: &HashSet<&str>) {
        for field in &mut self.fields {
            let key = required.contains(field.name());
            field.set_required(key);
        }
    }

    /// Renders the schema from `source` and fails with declaration errors instead of rendering.
    ///
    /// # Errors
    ///
    /// A misdeclared schema fails with its errors rather than render.
    pub async fn render<'a>(&self, cx: &'a Cx, source: Source<'_>) -> Result<BoxView<'a>> {
        let errors = self.declaration_errors();
        if !errors.is_empty() {
            return Err(crate::error::misdeclared(&errors));
        }
        let source = source.watching(self.watched());
        render_nodes(cx, &self.nodes, &self.fields, &source).await
    }

    /// Appends `other`'s nodes and fields after this one's, re-slotting its field slots.
    pub(crate) fn append(&mut self, other: Schema<F>) {
        let Schema {
            mut nodes, fields, ..
        } = other;
        let offset = self.fields.len();
        for node in &mut nodes {
            node.offset(offset);
        }
        self.fields.extend(fields);
        self.nodes.extend(nodes);
    }

    /// The embedded node a derived value's schema holds.
    pub(crate) fn embedded_root(&self) -> &embedded::Embedded {
        match self.nodes.as_slice() {
            [Node::Embedded(node)] => node,
            _ => panic!("an embedded value's schema is its one embedded node"),
        }
    }

    /// The create form's initial values: each field's `default`.
    pub(crate) fn defaults(&self) -> FormValues {
        self.fields
            .iter()
            .filter_map(|field| Some((field.name().to_string(), field.default_value()?.clone())))
            .collect()
    }

    /// The `default` of each field `keys` names that declares one.
    pub(crate) fn defaults_of<'k>(&self, keys: impl Iterator<Item = &'k str>) -> FormValues {
        let keys: Vec<&str> = keys.collect();
        self.fields
            .iter()
            .filter(|field| keys.contains(&field.name()))
            .filter_map(|field| Some((field.name().to_string(), field.default_value()?.clone())))
            .collect()
    }

    /// Drops what a disabled control posted, which the browser never sends: on the edit form when
    /// `editing`, else on the create form, where the field takes its `default`. Run it before
    /// [`condition_hidden`](Self::condition_hidden), which drops a hidden field's default again.
    pub(crate) fn drop_disabled(&self, values: &mut FormValues, editing: bool) {
        for field in self
            .fields
            .iter()
            .filter(|field| field.is_disabled(editing))
        {
            let name = field.name();
            values.remove(name);
            if let Some(default) = field.default_value()
                && !editing
            {
                values.insert(name, default.clone());
            }
        }
    }

    /// The keys of the controls disabled on the edit form when `editing`, else the create form.
    pub(crate) fn disabled_keys(&self, editing: bool) -> impl Iterator<Item = &str> {
        self.fields
            .iter()
            .filter(move |field| field.is_disabled(editing))
            .map(Field::name)
    }

    /// Appends another schema's nodes after this one's.
    pub fn extend(mut self, other: Schema<F>) -> Schema<F> {
        self.append(other);
        self
    }

    /// Returns the first key in `errors` that no control renders a message under: neither a
    /// field's own key nor, for a repeater, a row's key under `{key}.`.
    pub(crate) fn unplaced_error<'e>(&self, errors: &'e crate::FieldErrors) -> Option<&'e str> {
        errors.iter().map(|error| error.key.as_str()).find(|key| {
            !self
                .fields
                .iter()
                .any(|field| match key.strip_prefix(field.name()) {
                    Some(rest) => {
                        rest.is_empty() || (rest.starts_with('.') && field.as_repeater().is_some())
                    }
                    None => false,
                })
        })
    }

    /// One [`EmptyChoice`](crate::DeclarationErrorKind::EmptyChoice) per choice with neither
    /// options nor a relationship. Mounting checks a resource's form and its actions' inputs; a
    /// page's schema may build its options from data that is empty for now, so rendering does
    /// not.
    pub(crate) fn empty_choices(&self) -> Vec<crate::DeclarationErrorKind> {
        self.fields
            .iter()
            .filter(|field| field.offers_nothing())
            .map(|field| crate::DeclarationErrorKind::EmptyChoice {
                field: field.name().to_string(),
            })
            .collect()
    }

    /// What is wrong with the schema's dependent choices: a column of another model than the
    /// relationship's source, or no relationship, and a parent the schema does not place, which
    /// posts nothing to narrow the options by.
    fn dependent_errors(&self) -> Vec<crate::DeclarationErrorKind> {
        let mut errors = Vec::new();
        for field in &self.fields {
            let Some(choice) = field.as_choice() else {
                continue;
            };
            if choice.misdeclared_parent() {
                errors.push(crate::DeclarationErrorKind::MisdeclaredDependentChoice {
                    field: field.name().to_string(),
                });
            }
            let Some(parent) = choice.parent_key() else {
                continue;
            };
            match self.fields.iter().find(|placed| placed.name() == parent) {
                None => errors.push(crate::DeclarationErrorKind::UnplacedParentField {
                    field: field.name().to_string(),
                    parent: parent.to_string(),
                }),
                Some(parent)
                    if choice.is_multiple()
                        || parent.as_choice().is_some_and(ChoiceControl::is_multiple) =>
                {
                    errors.push(crate::DeclarationErrorKind::MultipleDependentChoice {
                        field: field.name().to_string(),
                    });
                }
                Some(_) if choice.is_listed() => {
                    errors.push(crate::DeclarationErrorKind::ListedDependentChoice {
                        field: field.name().to_string(),
                    });
                }
                Some(_) => {}
            }
        }
        errors
    }

    /// Each key a condition, a dependent choice or a comparison watches, with the key of a field
    /// it drives: `None` for a condition guarding no field, such as one over an empty section.
    pub(crate) fn watchers(&self) -> Vec<(String, Option<String>)> {
        let mut watchers = self.condition_watchers();
        watchers.extend(self.comparison_watchers());
        watchers.extend(self.fields.iter().filter_map(|field| {
            field
                .parent_key()
                .map(|parent| (parent.to_string(), Some(field.name().to_string())))
        }));
        watchers
    }

    /// Reports what is wrong with this declaration: a field whose lens binds no single column, and
    /// two fields sharing a name.
    pub fn declaration_errors(&self) -> Vec<crate::DeclarationErrorKind> {
        let mut errors = self.condition_errors();
        errors.extend(self.dependent_errors());
        errors.extend(self.comparison_errors(""));
        // A control disabled on create posts nothing there, so the create needs a value.
        for field in &self.fields {
            errors.extend(field.rule_errors(field.name()));
            if let Some(repeater) = field.as_repeater() {
                let item = repeater.item_schema();
                for leaf in item.fields() {
                    errors.extend(leaf.rule_errors(&format!("{}.{}", field.name(), leaf.name())));
                }
                errors.extend(item.comparison_errors(&format!("{}.", field.name())));
            }
            let blank = field.default_value().is_none_or(|value| match value {
                FormValue::Text(text) => text.trim().is_empty(),
                FormValue::List(items) => items.is_empty(),
                FormValue::Rows(rows) => rows.is_empty(),
            });
            if field.is_always_disabled() && field.is_required() && blank {
                errors.push(crate::DeclarationErrorKind::DisabledWithoutValue {
                    field: field.name().to_string(),
                });
            }
            if let Some(value) = field.default_value()
                && field.refuses(value)
            {
                errors.push(crate::DeclarationErrorKind::UnpostedDefault {
                    field: field.name().to_string(),
                    value: match value {
                        FormValue::Text(text) => text.clone(),
                        FormValue::List(items) => format!("{items:?}"),
                        FormValue::Rows(_) => "rows".to_string(),
                    },
                });
            }
            // A row's controls post under the repeater's key, which neither the disabled drop nor
            // the defaults read.
            if let Some(repeater) = field.as_repeater()
                && let Some(leaf) = repeater
                    .item_schema()
                    .fields()
                    .find(|leaf| leaf.is_disabled(true) || leaf.default_value().is_some())
            {
                errors.push(crate::DeclarationErrorKind::RepeaterLeafModifier {
                    field: field.name().to_string(),
                    leaf: leaf.name().to_string(),
                });
            }
            // An edit renders each stored row, and a password never renders back: every save
            // would replace the stored secret with what the user retyped, or with nothing.
            if let Some(repeater) = field.as_repeater()
                && let Some(leaf) = repeater
                    .item_schema()
                    .fields()
                    .find(|leaf| leaf.is_password())
            {
                errors.push(crate::DeclarationErrorKind::RepeaterPassword {
                    field: field.name().to_string(),
                    leaf: leaf.name().to_string(),
                });
            }
        }
        let mut seen = HashSet::new();
        for field in &self.fields {
            match field.misdeclared() {
                Some(error) => errors.push(error),
                None if !seen.insert(field.name()) => {
                    errors.push(crate::DeclarationErrorKind::DuplicateField {
                        name: field.name().to_string(),
                    });
                }
                None => {}
            }
        }
        errors
    }

    /// Collects the keys of the fields this submission hides: the payload of every embedded
    /// variant it does not choose, and every field a condition hides.
    pub(crate) fn hidden_fields(&self, values: &FormValues) -> HashSet<String> {
        fn walk(nodes: &[Node], values: &FormValues, out: &mut Vec<usize>) {
            for node in nodes {
                match node {
                    Node::Embedded(embedded) => embedded.hidden_fields(values, out),
                    node => {
                        if let Some(children) = node.children() {
                            walk(children, values, out);
                        }
                    }
                }
            }
        }
        let mut indices = Vec::new();
        walk(&self.nodes, values, &mut indices);
        indices
            .into_iter()
            .map(|index| self.fields[index].name().to_string())
            .chain(self.condition_hidden(values))
            .collect()
    }

    /// Adds to `errors` what the controls' own rules refuse in a submission: an unchecked
    /// accepted checkbox, a multiple choice's or a repeater's count of items, an email or URL
    /// field's value, a text field's declared length, bounds and pattern, and a choice that is not
    /// one of its options. Skips hidden fields and keys `errors` already refuses. A value the
    /// record already stores keeps its rules unchecked: a disabled control posts it back, and a
    /// rule declared since must not lock the record.
    pub(crate) async fn check_controls(
        &self,
        cx: &Cx,
        values: &FormValues,
        stored: &FormValues,
        errors: &mut FieldErrors,
    ) {
        let hidden = self.hidden_fields(values);
        let zone = crate::time_zone(cx);
        for field in &self.fields {
            let name = field.name();
            if errors.contains_key(name) || hidden.contains(name) {
                continue;
            }
            let Some(value) = values.get(name) else {
                if let Some(error) = field.check_unposted(&zone) {
                    errors.push(error);
                }
                continue;
            };
            let refusal = (stored.get(name) != Some(value))
                .then(|| field.check(value, &zone))
                .flatten();
            if let Some(repeater) = field.as_repeater() {
                // The count's refusal owns the repeater's key; each row's own keys still check.
                if let Some(error) = refusal {
                    errors.push(error);
                }
                let rows = value.as_rows().unwrap_or_default();
                let held = stored.get(name).and_then(FormValue::as_rows);
                repeater
                    .check(cx, name, rows, held.unwrap_or_default(), errors)
                    .await;
                continue;
            }
            if let Some(error) =
                refusal.or_else(|| self.comparison_refusal(field, values, stored, &hidden))
            {
                errors.push(error);
                continue;
            }
            let parent = field.parent_key().and_then(|key| values.text(key));
            for message in field
                .validate_exists(cx, value, parent, stored.get(name))
                .await
            {
                errors.add(name, message);
            }
        }
    }

    /// What the controls' own rules refuse in `values`, alone.
    #[cfg(test)]
    pub(crate) async fn checked(&self, cx: &Cx, values: &FormValues) -> FieldErrors {
        let mut errors = FieldErrors::new();
        self.check_controls(cx, values, &FormValues::new(), &mut errors)
            .await;
        errors
    }

    /// Puts back the stored value of each text field whose control posted it as rendered: see
    /// [`Field::keep_unedited`].
    pub(crate) fn keep_unedited(
        &self,
        values: &mut FormValues,
        stored: &FormValues,
        zone: &jiff::tz::TimeZone,
    ) {
        for field in &self.fields {
            let name = field.name();
            let (Some(value), Some(held)) = (values.get(name), stored.get(name)) else {
                continue;
            };
            if let Some(kept) = field.keep_unedited(value, held, zone) {
                values.insert(name, kept);
            }
        }
    }

    /// Re-checks every submitted relationship key through the write's open transaction, but a
    /// multiple choice's keys the record's stored value in `stored` holds already.
    pub(crate) async fn recheck_relationships(
        &self,
        cx: &Cx,
        values: &FormValues,
        stored: &FormValues,
        ex: &mut dyn toasty::Executor,
    ) -> FieldErrors {
        let mut errors = FieldErrors::new();
        let hidden = self.hidden_fields(values);
        for field in &self.fields {
            let name = field.name();
            if hidden.contains(name) {
                continue;
            }
            let Some(value) = values.get(name) else {
                continue;
            };
            let parent = field.parent_key().and_then(|key| values.text(key));
            for message in field
                .recheck(cx, value, parent, stored.get(name), &mut *ex)
                .await
            {
                errors.add(name, message);
            }
        }
        errors
    }

    /// Puts back in each multiple choice's list the keys of the record's stored list `stored` it
    /// drops that the user cannot view, through the write's open transaction: the form never
    /// offered them, so its submission leaves them linked.
    pub(crate) async fn keep_unseen_links(
        &self,
        cx: &Cx,
        values: &mut FormValues,
        stored: &FormValues,
        ex: &mut dyn toasty::Executor,
    ) {
        for field in &self.fields {
            let name = field.name();
            let (Some(value), Some(held)) = (values.get(name), stored.get(name)) else {
                continue;
            };
            if let Some(kept) = field.keep_unseen(cx, value, held, &mut *ex).await {
                values.insert(name, kept);
            }
        }
    }
}

#[cfg(test)]
mod tests;
