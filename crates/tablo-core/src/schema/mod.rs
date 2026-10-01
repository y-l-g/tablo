//! Unified Schema primitive — fields and layout blocks that compose via `view!`.
//!
//! `Schema` holds a tree of `Section`, `Group`, `Grid`, and `Repeater`
//! blocks, embedded values, and [`Field`] slots. Building it resolves every
//! field once into one list, which rendering, validation, and the panel's
//! checks read. The API mirrors Filament's `Schema::new(( ... ))` tuple form
//! via the `IntoSchema` trait.
//!
//! Bridge note: `lens_field` is the one walk reaching into `toasty_core`
//! (upstream issue #114), alongside the `pk_*` bridge helpers and `cursor.rs`
//! cursor values. It hands back the built `app::Field`, so field metadata
//! needs no helper per property; uniqueness comes from `lens_field_unique`,
//! since Toasty keeps it on the model's index list rather than the field.
//! Retire the walk when Toasty exposes it (upstream #183).

pub(crate) mod embedded;
mod fields;
mod layouts;
mod lenses;
mod options;
mod pk;
mod relationship;
mod tree;
mod validation;

use std::collections::{HashMap, HashSet};

pub use embedded::EmbeddedForm;
pub(crate) use fields::option_view;
pub use fields::{
    ChoiceField, Control, ControlInput, CustomField, Field, FileField, IntoOptions, TextField,
    Toggle,
};
pub use layouts::{Grid, Group, Repeater, Section};
pub use lenses::{DeclCx, FieldLens, ResolvedLens};
pub(crate) use lenses::{LensBinding, capitalize, lens_field, lens_field_unique};
pub use options::Options;
pub(crate) use pk::{pk_eq_expr, pk_in_expr, pk_is_composite};
pub(crate) use relationship::OptionLoadError;
pub use relationship::{MAX_RELATIONSHIP_OPTIONS, OptionSource};
use topcoat::{Result, context::Cx, view::*};
pub use tree::{IntoSchema, Source};
pub(crate) use tree::{LeafPlace, Node, render_nodes, walk_absent_groups};
pub use validation::TypedValue;
pub(crate) use validation::required_error;

use crate::form::FieldErrors;

/// One control as the record-form checks see it ([`Schema::controls`]).
#[derive(Debug, Clone)]
pub(crate) struct ControlCheck {
    /// The key the control posts.
    pub(crate) name: String,
    /// Whether an empty submission fails the control's rules.
    pub(crate) required: bool,
    /// The message an empty submission produces, when it fails.
    pub(crate) required_error: Option<String>,
    /// Whether the control sits inside a `Repeater`, whose all-empty group
    /// skips its requiredness: a submission may then reach the parse with the
    /// control empty.
    pub(crate) in_repeater: bool,
    /// Where the control sits in the form ([`LeafPlace`]).
    pub(crate) place: LeafPlace,
}

impl ControlCheck {
    /// Whether an empty submission reaches this control's rule.
    ///
    /// A rendered control always does; a variant group's payload does when the
    /// group sits inside a `Repeater`, whose absent group skips requiredness
    /// while the parse still reads the payload (a hidden group is not read at
    /// all, and a live one is the parse's refused blank to word); an enum's
    /// discriminant never does, because an empty one reaches the payload
    /// fallback.
    pub(crate) fn needs_answer(&self) -> bool {
        match self.place {
            LeafPlace::Rendered => true,
            LeafPlace::Payload => self.in_repeater,
            LeafPlace::Discriminant => false,
        }
    }
}

/// The container that composes fields and layout blocks.
///
/// The field list is resolved once, when the schema is built: every
/// [`Field`] a block or an embedded value holds moves into it, and the tree
/// holds slots into it.
#[derive(Debug, Default)]
pub struct Schema {
    pub(crate) nodes: Vec<Node>,
    pub(crate) fields: Vec<Field>,
}

impl Schema {
    /// Whether this schema declares nothing to render.
    ///
    /// Public because [`Resource::view`](crate::resource::Resource::view) defaults to
    /// an empty schema, which the panel reads as "no detail page declared".
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Build a `Schema` from any `IntoSchema` (a field, a builder, a block,
    /// a tuple, or a `Schema`).
    ///
    /// Two fields sharing one name are a misdeclaration
    /// ([`Self::declaration_errors`]): they render two `<input name="x">`,
    /// POST one value to both, and collapse to one validation rule.
    pub fn new(children: impl IntoSchema) -> Self {
        children.into_schema()
    }

    /// An empty schema (no nodes).
    pub fn empty() -> Self {
        Self::default()
    }

    /// Every field, in declaration order.
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter()
    }

    /// Render the schema from `source`: a form's controls
    /// ([`Source::form`]) or a record's read-only values ([`Source::view`]).
    ///
    /// # Errors
    ///
    /// A misdeclared schema ([`Self::declaration_errors`]) fails with its
    /// errors rather than render.
    pub async fn render<'a>(&self, cx: &'a Cx, source: Source<'_>) -> Result<BoxView<'a>> {
        let errors = self.declaration_errors();
        if !errors.is_empty() {
            return Err(crate::error::TabloError::Declaration(errors.join("; ")).into());
        }
        render_nodes(cx, &self.nodes, &self.fields, &source).await
    }

    /// Append `other`'s nodes and fields after this one's, re-slotting its
    /// field slots past this schema's fields.
    pub(crate) fn append(&mut self, other: Schema) {
        let Schema { mut nodes, fields } = other;
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

    /// Rewrite submitted values into their fields' stored spelling.
    ///
    /// Runs after validation and before a record fn sees the map, so a typed
    /// field's `Display` — not the browser's spelling — is what gets written.
    /// That is what makes an untouched edit round-trip: the form hydrates a
    /// stored value, the browser echoes it, and this puts back the same string
    /// the record fn would have produced.
    ///
    /// A choice takes its trimmed submission: its presence rule and its
    /// option-existence check both read `value.trim()`, so the trimmed value is
    /// the one that passed, and storing the untrimmed spelling would store a
    /// value no rule authorised.
    ///
    /// A parse failure here leaves the submission untouched and reports nothing:
    /// it is unreachable from the handlers, and a silent rewrite would hide a
    /// bypass rather than surface it. A field with no submission keeps its
    /// absence, and an **empty** submission stays empty: a typed column has no
    /// spelling for "no value", so empty is the presence rule's business —
    /// `.required()` refuses it inline, and an optional typed field reaches the
    /// record form's parse as `""`, which reads it as the field's blank answer.
    pub fn normalize_values(&self, values: &mut HashMap<String, String>) {
        for field in &self.fields {
            let Some(submitted) = values.get_mut(field.name()) else {
                continue;
            };
            if submitted.trim().is_empty() {
                continue;
            }
            if let Ok(normalized) = field.normalize(submitted) {
                *submitted = normalized;
            }
        }
    }

    /// Append another schema's nodes after this one's.
    ///
    /// [`Schema::new`] composes through `IntoSchema`, whose tuple form stops at
    /// eight nodes; a longer form appends instead. The nodes keep their order,
    /// so a form reads in declaration order either way.
    pub fn extend(mut self, other: Schema) -> Schema {
        self.append(other);
        self
    }

    /// Every field, in declaration order, with what an empty submission
    /// does to it: the field's own required message when it refuses one, and
    /// where its control sits — inside an all-empty `Repeater`, whose group
    /// skips requiredness ([`walk_absent_groups`]), or where a submission can
    /// skip the control at all.
    pub(crate) fn controls(&self) -> Vec<ControlCheck> {
        fn mark(nodes: &[Node], inside: bool, in_repeater: &mut [bool], place: &mut [LeafPlace]) {
            for node in nodes {
                match node {
                    Node::Repeater(r) => mark(&r.children.nodes, true, in_repeater, place),
                    Node::Field(_) | Node::Embedded(_) => {
                        node.visit_fields(&mut |index, leaf_place| {
                            in_repeater[index] = inside;
                            place[index] = leaf_place;
                        });
                    }
                    _ => mark(
                        node.children().unwrap_or_default(),
                        inside,
                        in_repeater,
                        place,
                    ),
                }
            }
        }
        let mut in_repeater = vec![false; self.fields.len()];
        let mut place = vec![LeafPlace::Rendered; self.fields.len()];
        mark(&self.nodes, false, &mut in_repeater, &mut place);
        self.fields
            .iter()
            .zip(in_repeater)
            .zip(place)
            .map(|((field, in_repeater), place)| {
                let errors = field.validate("");
                ControlCheck {
                    name: field.name().to_string(),
                    required: !errors.is_empty(),
                    // A control no rule makes required has no declared wording;
                    // the label names it for the parse's own refusal, which an
                    // embedded leaf can reach where the check exempts it.
                    required_error: errors
                        .into_iter()
                        .next()
                        .map(|error| error.message)
                        .or_else(|| Some(required_error(field.label_str()))),
                    in_repeater,
                    place,
                }
            })
            .collect()
    }

    /// Keys in `values` that no declared input owns, sorted.
    ///
    /// Framework-level allow-list seam, enforced by the create/edit POST
    /// handlers (unknown keys → 400): record handlers already whitelist via
    /// per-field `.get(..)`, but a generic impl iterating `values` would
    /// silently promote `role`/`tenant_id`/handler keys (`csrf_token`,
    /// `confirm`, `ids`) to client-controlled writes. The transport keys the
    /// handlers own (`csrf_token`, `clear_<field>`, `keep_<field>`) are
    /// stripped before the record fns run, so a generic impl cannot
    /// promote those either; `confirm`/`ids` are only read, never written.
    /// Callers should reject or ignore the rest (at least `debug_assert!` in
    /// tests); handler keys must be filtered by the caller before calling this.
    pub fn unknown_keys(&self, values: &HashMap<String, String>) -> Vec<String> {
        let known: HashSet<&str> = self.fields.iter().map(Field::name).collect();
        let mut out: Vec<String> = values
            .keys()
            .filter(|k| !known.contains(k.as_str()))
            .cloned()
            .collect();
        out.sort();
        out
    }

    /// What is wrong with this declaration: a field whose lens binds no
    /// single column, and two fields sharing a name.
    ///
    /// [`RouterBuilderPanelExt::panel`](crate::RouterBuilderPanelExt::panel) refuses a resource
    /// whose form or view reports any, and rendering one fails with them.
    pub fn declaration_errors(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut seen = HashSet::new();
        for field in &self.fields {
            match field.misdeclared() {
                Some(error) => errors.push(format!("field `{}`: {error}", field.name())),
                None if !seen.insert(field.name()) => errors.push(format!(
                    "duplicate field name '{}': each Schema input needs a distinct field",
                    field.name()
                )),
                None => {}
            }
        }
        errors
    }

    /// Validate submitted values against declared inputs.
    ///
    /// Absent keys are treated as `""`. The edit handler completes every key
    /// the submission did not post from the stored record before it validates,
    /// so an omitted key validates as its stored value there
    /// ([`Resource::update_record`](crate::Resource::update_record)). Use [`Self::unknown_keys`] to
    /// allow-list POST keys.
    ///
    /// A field a submission hides is not validated: an all-empty
    /// Repeater group is absent, and a variant group the submission's
    /// discriminant does not name is not rendered by `variant.js`, so neither
    /// can fail the submit for a value the user cannot see.
    pub fn validate(&self, values: &HashMap<String, String>) -> FieldErrors {
        // Classify the groups first: an all-empty group is
        // "absent" — an untouched group submits empty strings (or omits the
        // keys), both treated as absent — so its inner inputs must not fail
        // the submit for any requiredness. A `required` repeater answers with
        // its one label-keyed error instead, and required repeaters nested
        // inside an absent group are suppressed with it. A partially filled
        // group (any inner value non-empty) enforces inner `required` as
        // usual. Whitespace-only values count as empty, matching the
        // codebase-wide trim convention. A variant group the submission's
        // discriminant does not name is hidden with its subtree, which is what
        // makes validation agree with the render.
        let mut errors = FieldErrors::new();
        let mut skip: HashSet<String> = HashSet::new();
        walk_absent_groups(
            &self.nodes,
            &self.fields,
            values,
            &mut skip,
            &mut errors,
            false,
        );
        for field in &self.fields {
            if skip.contains(field.name()) {
                continue;
            }
            let value = values.get(field.name()).map(String::as_str).unwrap_or("");
            for error in field.validate(value) {
                errors.push(error);
            }
        }
        errors
    }

    /// Field names a submission leaves out of validation:
    /// the same classification `validate` uses — an all-empty Repeater group
    /// is absent, and a variant group the discriminant does not name is hidden
    /// — minus the required-group errors, which validation already reported.
    /// `check_unique` consults it so an untouched group is never unique-checked
    /// while validation calls it clean, and `validate_async` so a hidden
    /// group's select is not probed for existence.
    pub(crate) fn absent_fields(&self, values: &HashMap<String, String>) -> HashSet<String> {
        let mut skip = HashSet::new();
        let mut discarded = FieldErrors::new();
        walk_absent_groups(
            &self.nodes,
            &self.fields,
            values,
            &mut skip,
            &mut discarded,
            false,
        );
        skip
    }

    /// Whether a render reads an error under `key` for this submission: a
    /// field's own name, or a repeater group's label — the two keys
    /// [`Source::errors_for`] reads — and not a field a variant group the
    /// submission's discriminant does not name hides. A submission naming no
    /// discriminant hides nothing, because the payload may name the variant.
    ///
    /// A key no slot owns is one the submit handlers refuse as a declaration
    /// error rather than block the write behind an error the form cannot place.
    pub(crate) fn renders_error_key(&self, values: &HashMap<String, String>, key: &str) -> bool {
        // A repeater's own error slot is keyed by its label (see
        // `walk_absent_groups`), which no field carries.
        fn labels(nodes: &[Node], key: &str) -> bool {
            nodes.iter().any(|node| match node {
                Node::Repeater(repeater) => {
                    repeater.label == key || labels(&repeater.children.nodes, key)
                }
                node => node
                    .children()
                    .is_some_and(|children| labels(children, key)),
            })
        }
        // Every field of the compiled list renders somewhere, except a leaf of a
        // variant group this submission hides.
        let hidden = self.hidden_fields(values);
        let renders_field = self
            .fields
            .iter()
            .any(|field| field.name() == key && !hidden.contains(key));
        renders_field || labels(&self.nodes, key)
    }

    /// Field names this submission hides: every leaf of a variant group its
    /// discriminant does not name (the classification `absent_fields` shares).
    fn hidden_fields(&self, values: &HashMap<String, String>) -> HashSet<String> {
        fn walk(nodes: &[Node], values: &HashMap<String, String>, out: &mut Vec<usize>) {
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
            .collect()
    }

    /// [`Self::validate`], then each choice's option existence
    /// (tenancy-aware for a relationship).
    ///
    /// A field `validate` skipped is skipped here too: an absent
    /// repeater group or a hidden variant group holds no value the user can
    /// see, so its choice must not be probed for existence.
    pub async fn validate_async(&self, cx: &Cx, values: &HashMap<String, String>) -> FieldErrors {
        let mut errors = self.validate(values);
        let absent = self.absent_fields(values);
        for field in &self.fields {
            let name = field.name();
            if errors.contains_key(name) || absent.contains(name) {
                continue;
            }
            let Some(value) = values.get(name) else {
                continue;
            };
            // `validate` above already ran the required rule, so the
            // existence-only check is what is left to ask.
            for message in field.validate_exists(cx, value).await {
                errors.add(name, message);
            }
        }
        errors
    }

    /// Re-check every submitted relationship key through `ex`, the write's
    /// open transaction, skipping what [`Self::validate_async`] skips.
    ///
    /// The write handlers run it after they open the transaction, so a record
    /// a relationship field points at is checked against the rows the write
    /// sees: one that left the tenant, was deleted or became hidden since the
    /// form validated refuses the write with a field error, and nothing is
    /// written.
    pub(crate) async fn recheck_relationships(
        &self,
        cx: &Cx,
        values: &HashMap<String, String>,
        ex: &mut dyn toasty::Executor,
    ) -> FieldErrors {
        let mut errors = FieldErrors::new();
        let absent = self.absent_fields(values);
        for field in &self.fields {
            let name = field.name();
            if absent.contains(name) {
                continue;
            }
            let Some(value) = values.get(name) else {
                continue;
            };
            for message in field.recheck(cx, value, &mut *ex).await {
                errors.add(name, message);
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests;
