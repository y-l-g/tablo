use tablo_ui::input as ui_input;
use topcoat::{Result, context::Cx, view::*};

use super::{
    super::{
        lenses::{FieldResolver, lens_field, lens_field_unique, lens_label},
        tree::Mode,
        validation::{
            Rules, TypedValue, format_timestamp_input, is_timestamp, parse_timestamp_storage,
        },
    },
    FieldChrome, ValueKind, render_field, render_value,
};

/// The equality expression a typed leaf's unique probe binds.
///
/// Built where the declared type is known — the lens constructor — so a typed
/// field compares as its declared type rather than as its text. `None` means
/// the submitted value does not parse into that type: validation has already
/// refused it, and the probe has nothing to compare.
type EqProbe = std::sync::Arc<dyn Fn(&str) -> Option<toasty::stmt::Expr<bool>> + Send + Sync>;

/// The equality probe a typed leaf binds: the submission is parsed into the
/// declared type and compared through that type's own path, so a value that is
/// unique as text but not as the type (or the reverse) is checked for what the
/// record will store.
///
/// The index resolves inside the closure rather than here: the name is the
/// leaf's **app field name**, and for a context-bound leaf that is its flattened
/// storage column (`seo_title`), which the leaf's own lens root does not name —
/// toasty matches app field names — so resolving it eagerly would panic for an
/// embedded typed leaf. Only a `unique()` marker reaches the closure.
///
/// `IntoExpr` is what lets the comparison name the value: it is implemented for
/// every scalar toasty stores, and the panel's typed constructors require it
/// for the same reason they require `TypedValue` — a value that cannot become an
/// expression cannot be compared against its column.
fn eq_probe_typed<M, T>(name: &str) -> EqProbe
where
    M: toasty::schema::Model,
    T: TypedValue + toasty::stmt::IntoExpr<T> + 'static,
{
    let name = name.to_string();
    let timestamp = is_timestamp::<T>();
    std::sync::Arc::new(move |value: &str| {
        // A timestamp submission may be the control's zone-less shape;
        // validation normalises it to storage spelling first.
        if timestamp {
            let storage = parse_timestamp_storage(value).ok()?;
            let parsed = storage.parse::<T>().ok().filter(T::accepts)?;
            let index = M::field_name_to_id(&name).index;
            return Some(M::path_field::<T>(index).eq(parsed));
        }
        let parsed = value.parse::<T>().ok().filter(T::accepts)?;
        let index = M::field_name_to_id(&name).index;
        Some(M::path_field::<T>(index).eq(parsed))
    })
}

/// Typed text field bound to a Toasty field lens. The lens is the single
/// source of truth for the field name and type, so `TextInput::for(User::fields().name())`
/// fails to compile if the column does not exist (ADR-0001).
#[derive(Clone)]
pub struct TextInput {
    name: String,
    label: String,
    required: bool,
    unique: bool,
    placeholder: Option<String>,
    /// The email and typed-parse rules, with their messages.
    rules: Rules,
    /// The typed leaf's unique probe, absent on a text leaf: a text
    /// leaf's comparison is built from the model the handler queries.
    typed_probe: Option<EqProbe>,
    /// The control's `type`: `datetime-local` for a timestamp leaf, `text`
    /// otherwise (`email` overrides at render). A timestamp submits a
    /// zone-less `datetime-local` string the typed rule reads as UTC.
    input_type: &'static str,
}

impl std::fmt::Debug for TextInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The parser is a closure with no useful Debug; everything a reader
        // needs is the field's identity and which rules it declares.
        f.debug_struct("TextInput")
            .field("name", &self.name)
            .field("label", &self.label)
            .field("required", &self.required)
            .field("is_email", &self.rules.is_email())
            .field("unique", &self.unique)
            .field("placeholder", &self.placeholder)
            .field("typed", &self.rules.is_typed())
            .field("input_type", &self.input_type)
            .finish()
    }
}

impl TextInput {
    /// Create a `TextInput` bound to the given field lens.
    ///
    /// Only `String` lenses compile: binding a non-text field (a `Uuid` key,
    /// a `bool`, …) fails at compile time, mirroring `TextColumn`.
    ///
    /// `required` and `unique` default from the field's metadata (GH #100,
    /// GH #183): a non-nullable column is required, and a field backed by a
    /// single-field unique index is unique — so neither has to be restated by
    /// hand. Both stay overridable with `.optional()` / `.unique()`.
    pub fn r#for<M>(path: toasty::stmt::Path<M, String>) -> Self
    where
        M: toasty::schema::Model,
    {
        let model = M::schema();
        let field = lens_field(path, &model);
        let label_str = lens_label(&field);
        let unique = lens_field_unique(&field, model.as_root_unwrap());
        let name = field.name.app_unwrap().to_string();
        Self {
            name,
            label: label_str,
            // Non-nullable columns are required by default: an
            // empty submit would die at the driver instead of failing
            // inline. Override with `.optional()` for nullable columns.
            required: !field.nullable(),
            unique,
            placeholder: None,
            rules: Rules::new(),
            typed_probe: None,
            input_type: "text",
        }
    }

    /// Create a `TextInput` bound to a lens inside an embedded struct or a
    /// `#[document]`.
    ///
    /// The plain `Self::r#for` resolves a lens against the model alone, which
    /// is why it can only bind a top-level field: the owned `app::Model` cannot
    /// see the embedded models, so a path like `Post::fields().seo().title()`
    /// is rejected as a traversal lens. This resolves through the request's app
    /// schema instead, so the leaf arrives as its **flattened storage column**
    /// (`seo_title`) — the name the form posts and the record fn reads.
    ///
    /// An embedded leaf is never `required` by default: the resolver reports
    /// `nullable=true` by binding policy, since only the matching enum variant
    /// writes a variant payload column. That is the binding default, not a
    /// storage fact — the flattened column of a required embedded struct is
    /// `NOT NULL`. Opt in with [`.required()`](Self::required).
    ///
    /// Without a `Db` in context (a bare `CxTestBuilder`) this behaves exactly
    /// like `Self::r#for` and rejects the traversal lens loudly, so a test
    /// cannot silently bind the wrong column.
    pub fn r#for_context<M>(cx: &Cx, path: toasty::stmt::Path<M, String>) -> Self
    where
        M: toasty::schema::Model,
    {
        let leaf = FieldResolver::from_cx(cx).resolve(path);
        Self {
            name: leaf.name,
            label: leaf.label,
            required: !leaf.nullable,
            unique: false,
            placeholder: None,
            rules: Rules::new(),
            typed_probe: None,
            input_type: "text",
        }
    }

    /// Create a `TextInput` bound to a lens whose leaf is **not** a `String`.
    ///
    /// `Self::r#for` takes a `Path<M, String>`, which makes a wrong lens a
    /// compile error rather than a runtime mismatch (ADR-0001) — and also makes
    /// a typed column unbindable. This constructor keeps that guarantee: the
    /// lens must address one field of the model and `T` must be the leaf's
    /// actual type, so `TextInput::typed::<User, Uuid>(User::fields().name())`
    /// does not compile either. It adds the value's spelling rule:
    ///
    /// - the control renders the value's `Display`;
    /// - a submission `T` cannot parse is an **inline field error** naming the offending input (``
    ///   `2024-13-01` is not a valid timestamp ``);
    /// - what is stored is `T`'s `Display` of the parsed value, so a value re-submitted unchanged
    ///   is written back in the shape it was read.
    ///
    /// The record fn still receives `String`s: the panel's value map is
    /// text-keyed, and a typed field is a *validated* string, not a second
    /// channel.
    ///
    /// `TypedValue` is implemented for the types a panel binds — the integer
    /// types, `bool`, `f32`, `f64`, `Uuid`, `jiff::Timestamp` — rather than as a
    /// blanket over `FromStr`, because the error names what was expected; `T`
    /// must be [`toasty::stmt::IntoExpr`] of itself so the unique probe compares
    /// through the parsed value rather than its text.
    ///
    /// A `jiff::Timestamp` leaf renders `type="datetime-local"`: the control
    /// carries no zone, so the stored instant renders in UTC and a submission
    /// is read back as UTC.
    pub fn typed<M, T>(path: toasty::stmt::Path<M, T>) -> Self
    where
        M: toasty::schema::Model,
        T: TypedValue + toasty::stmt::IntoExpr<T> + 'static,
    {
        let model = M::schema();
        let field = lens_field(path, &model);
        let label_str = lens_label(&field);
        // A typed field declares no index and is not marked unique by default;
        // when the app marks it, the probe binds the declared type.
        let name = field.name.app_unwrap().to_string();
        let probe = eq_probe_typed::<M, T>(&name);
        let input_type = if is_timestamp::<T>() {
            "datetime-local"
        } else {
            "text"
        };
        Self {
            name,
            label: label_str,
            required: !field.nullable(),
            unique: false,
            placeholder: None,
            rules: Rules::new().typed::<T>(),
            typed_probe: Some(probe),
            input_type,
        }
    }

    /// [`Self::typed`] for a lens inside an embedded struct or a `#[document]`,
    /// resolving through the request's app schema exactly as
    /// `Self::r#for_context` does.
    ///
    /// A leaf under an embedded step is never required by default: the resolver
    /// reports `nullable=true` by binding policy, since only the matching enum
    /// variant writes a variant payload column. That is the binding default,
    /// not a storage fact — the flattened column of a required embedded struct
    /// is `NOT NULL`. Opt in with [`.required()`](Self::required).
    pub fn typed_context<M, T>(cx: &Cx, path: toasty::stmt::Path<M, T>) -> Self
    where
        M: toasty::schema::Model,
        T: TypedValue + toasty::stmt::IntoExpr<T> + 'static,
    {
        let leaf = FieldResolver::from_cx(cx).resolve(path);
        let probe = eq_probe_typed::<M, T>(&leaf.name);
        let input_type = if is_timestamp::<T>() {
            "datetime-local"
        } else {
            "text"
        };
        Self {
            name: leaf.name,
            label: leaf.label,
            required: !leaf.nullable,
            unique: false,
            placeholder: None,
            rules: Rules::new().typed::<T>(),
            typed_probe: Some(probe),
            input_type,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Opt out of the non-nullable default: for nullable columns
    /// where an empty submit is legitimate.
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    pub fn email(mut self) -> Self {
        self.rules.set_email();
        self
    }

    /// Mark the field as backed by a unique constraint, which the app-side
    /// pre-check probes before the write.
    ///
    /// **Uniqueness implies presence**: the framework stores `""`,
    /// never NULL, so an empty value is one the index admits only
    /// once — an empty submit is refused inline as `"<Label> is required"`
    /// instead of being written, and the probe never sees it. `.optional()`
    /// does not lift that rule, whichever order the two are called in. The
    /// reasoning (and the rejected alternative) is recorded in the ADR-0010
    /// amendment of 2026-09-21.
    ///
    /// Non-`TextInput` fields declare no uniqueness (see `Textarea::r#for`),
    /// so nothing else changes.
    pub fn unique(mut self) -> Self {
        self.unique = true;
        // The marker carries presence itself, so `.optional().unique()` and
        // `.unique().optional()` mean the same thing.
        self.required = true;
        self
    }

    pub fn placeholder(mut self, p: impl Into<String>) -> Self {
        self.placeholder = Some(p.into());
        self
    }

    pub fn label(mut self, l: impl Into<String>) -> Self {
        self.label = l.into();
        self
    }

    pub fn is_unique(&self) -> bool {
        self.unique
    }

    /// Whether an empty submit fails validation and the control renders as
    /// required: `required`, defaulting from column nullability per GH #100,
    /// **or** uniqueness (GH #189 — a unique field is never empty, see
    /// [`Self::unique`]). `validate` and both render paths read it, so the rule
    /// and the marker cannot disagree.
    pub(crate) fn is_required(&self) -> bool {
        self.required || self.unique
    }

    pub fn field_name(&self) -> &str {
        &self.name
    }

    /// The human label (e.g. `"Email"`) — for inline error messages.
    pub fn label_str(&self) -> &str {
        &self.label
    }

    /// The equality expression the app-side unique check probes with.
    ///
    /// A text leaf compares its submission's text through `M`'s own path — the
    /// model the handler queries, which is also the model a context-bound
    /// leaf's flattened column belongs to. A typed leaf instead
    /// parses the submission into its declared type and compares that, so the
    /// probe sees the value the record will store rather than its spelling —
    /// `01` and `1` are one value to an integer column. `None` when a typed
    /// submission does not parse: validation has already refused it, and there
    /// is nothing left to compare.
    pub(crate) fn eq_filter<M>(&self, value: &str) -> Option<toasty::stmt::Expr<bool>>
    where
        M: toasty::schema::Model,
    {
        let Some(probe) = &self.typed_probe else {
            let index = M::field_name_to_id(&self.name).index;
            return Some(M::path_field::<String>(index).eq(value.to_string()));
        };
        probe(value)
    }

    /// Validate a raw string value against the configured rules.
    pub fn validate(&self, value: &str) -> Vec<String> {
        self.rules.validate(&self.label, self.is_required(), value)
    }

    /// The stored spelling of a submission the caller has already validated.
    ///
    /// The typed parse's `Display` for a typed field, the trimmed submission
    /// for a text one — so a value the user left alone is written back in the
    /// shape the record fn wrote it, not in whichever spelling the browser
    /// sent. Callers that have not validated must not use this: it reports a
    /// failure rather than guessing.
    pub(crate) fn normalize(&self, value: &str) -> Result<String, String> {
        self.rules.normalize(value)
    }

    /// Static render: the create/edit path's control, with `value` rendered
    /// into the `value` attribute and `errors` into the error slot.
    pub(crate) async fn render_with<'a>(
        &self,
        cx: &'a Cx,
        value: Option<&str>,
        errors: &[String],
        mode: Mode,
    ) -> Result<BoxView<'a>> {
        if mode == Mode::View {
            return render_value(cx, &self.label, value, ValueKind::Machine);
        }
        let name = self.name.clone();
        // The marker reads the same predicate validation uses, so a unique
        // field is never refused for emptiness while rendering as optional.
        // `self.required` alone would do exactly that.
        let required = self.is_required();
        let placeholder = self.placeholder.clone();
        let input_type = if self.rules.is_email() {
            "email"
        } else {
            self.input_type
        };
        // A timestamp renders its UTC `datetime-local` spelling, not the
        // stored RFC 3339: the control carries no zone.
        let value_owned = value.map(|s| {
            if self.input_type == "datetime-local" {
                format_timestamp_input(s)
            } else {
                s.to_string()
            }
        });
        let chrome = FieldChrome::new(&self.name, errors, None);
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome.described_by();
        // Beautiful rendering via the upstream `field` family (topcoat#420):
        // label + control + reserved error slot, the label following the
        // field's invalid state, and `aria-invalid` driving the control's
        // error border/ring.
        let control = view! {
            cx =>
            ui_input(
                attrs: attributes! {
                    id=(name.clone())
                    type=(input_type)
                    name=(name.clone())
                    value=(value_owned.clone())
                    placeholder=(placeholder.clone())
                    required=(required)
                    aria-required=(required.then_some("true"))
                    aria-invalid=(aria_invalid)
                    aria-describedby=(described_by)
                }
            )
        }
        .boxed();
        render_field(cx, &chrome, &self.label, required, attributes! {}, control)
    }
}

#[cfg(test)]
mod tests;
