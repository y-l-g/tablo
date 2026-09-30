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
mod pk;
mod relationship;
mod tree;
mod validation;

use std::collections::{HashMap, HashSet};

pub use embedded::EmbeddedForm;
pub use fields::Field;
pub use layouts::{Grid, Group, Repeater, Section};
pub use lenses::{FieldLens, ResolvedLens};
pub(crate) use lenses::{capitalize, lens_field, lens_field_unique, lens_label};
pub(crate) use pk::{pk_eq_expr, pk_in_expr, pk_is_composite};
pub(crate) use relationship::OptionLoadError;
pub use relationship::{MAX_RELATIONSHIP_OPTIONS, OptionSource};
use topcoat::{Result, context::Cx, view::*};
pub use tree::{IntoSchema, Source};
pub(crate) use tree::{Node, render_nodes, walk_absent_groups};
pub use validation::TypedValue;

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
    /// this and [`Resource::viewed`](crate::resource::Resource::viewed) reads it
    /// as "no detail page declared".
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Build a `Schema` from any `IntoSchema` (a field, a block, a tuple, or a
    /// `Schema`).
    ///
    /// Panics on duplicate field names: two inputs sharing one name
    /// render two `<input name="x">`, POST one value to both, and collapse to
    /// one validation rule.
    pub fn new(children: impl IntoSchema) -> Self {
        let schema = children.into_schema();
        schema.assert_unique_field_names();
        schema
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
    pub async fn render<'a>(&self, cx: &'a Cx, source: Source<'_>) -> Result<BoxView<'a>> {
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
        // The same guard `Schema::new` runs: this is the only check for the
        // shapes `new` cannot see.
        self.assert_unique_field_names();
        self
    }

    /// Every field, in declaration order, with what an empty submission
    /// does to it: the field's own required message when it refuses one, and
    /// whether an all-empty `Repeater` can skip its requiredness
    /// ([`walk_absent_groups`]).
    ///
    /// A variant group can skip requiredness too, but only an embedded value's
    /// fields sit in one, and its record-form field reads an empty key as
    /// `Default`, so the checks have nothing to ask of it.
    pub(crate) fn controls(&self) -> Vec<ControlCheck> {
        fn mark(nodes: &[Node], inside: bool, out: &mut [bool]) {
            for node in nodes {
                match node {
                    Node::Repeater(r) => mark(&r.children.nodes, true, out),
                    Node::Field(_) | Node::Embedded(_) => {
                        node.visit_fields(&mut |index, _| out[index] = inside)
                    }
                    _ => mark(node.children().unwrap_or_default(), inside, out),
                }
            }
        }
        let mut in_repeater = vec![false; self.fields.len()];
        mark(&self.nodes, false, &mut in_repeater);
        self.fields
            .iter()
            .zip(in_repeater)
            .map(|(field, in_repeater)| {
                let errors = field.validate("");
                ControlCheck {
                    name: field.name().to_string(),
                    required: !errors.is_empty(),
                    required_error: errors.into_iter().next(),
                    in_repeater,
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

    fn assert_unique_field_names(&self) {
        let mut seen = HashSet::new();
        for field in &self.fields {
            assert!(
                seen.insert(field.name()),
                "duplicate field name '{}': each Schema input needs a distinct field (GH #100)",
                field.name()
            );
        }
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
    pub fn validate(&self, values: &HashMap<String, String>) -> HashMap<String, Vec<String>> {
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
        let mut errors: HashMap<String, Vec<String>> = HashMap::new();
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
            let errs = field.validate(value);
            if !errs.is_empty() {
                errors.insert(field.name().to_string(), errs);
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
        let mut discarded = HashMap::new();
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

    /// [`Self::validate`], then each choice's option existence
    /// (tenancy-aware for a relationship).
    ///
    /// A field `validate` skipped is skipped here too: an absent
    /// repeater group or a hidden variant group holds no value the user can
    /// see, so its choice must not be probed for existence.
    pub async fn validate_async(
        &self,
        cx: &Cx,
        values: &HashMap<String, String>,
    ) -> HashMap<String, Vec<String>> {
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
            let existence = field.validate_exists(cx, value).await;
            if !existence.is_empty() {
                errors.insert(name.to_string(), existence);
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests;
