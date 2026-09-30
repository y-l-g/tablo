//! Field leaves: one [`Field`] type whose control is text, choice, or file.
//!
//! Every constructor takes a [`ResolvedLens`], which is the single source of
//! truth for the field's key, label, and required default, so a field of any
//! kind binds a top-level column or an embedded leaf alike.

mod choice;
mod file;
mod text;

pub(crate) use choice::{ChoiceControl, option_view};
use tablo_ui::{
    field as ui_field, field_content as ui_field_content, field_error as ui_field_error,
    field_label as ui_field_label, field_title as ui_field_title,
};
pub(crate) use text::TextControl;
use topcoat::{Result, context::Cx, view::*};

use super::{
    OptionSource,
    lenses::{ResolvedLens, capitalize},
    relationship::RelatedPrimaryKey,
    tree::Mode,
    validation::Rules,
};
use crate::form::FormScalar;

/// One form field: a key, a label, the rules a submission meets, and the
/// control that edits it.
///
/// Three constructors pick the control, and each binds any lens a
/// [`ResolvedLens`] accepts — a column (`Post::fields().title()`) or an
/// embedded leaf (`ResolvedLens::new(cx, Post::fields().seo().title())`):
///
/// ```ignore
/// Field::text(User::fields().name()).placeholder("Ada Lovelace")
/// Field::text(User::fields().email()).email().unique()
/// Field::text(User::fields().age())                  // typed: an `i64` column
/// Field::text(Post::fields().body()).multiline(6)
/// Field::choice(Post::fields().status()).options(vec!["draft".into(), "published".into()])
/// Field::choice(Post::fields().author_id()).relationship::<AuthorResource>(..)
/// Field::file(Doc::fields().path())
/// ```
///
/// `required` defaults from the column's nullability (GH #100): a
/// non-nullable column is required, so an empty submit fails inline instead of
/// at the driver. `.optional()` opts out and `.required()` opts back in.
///
/// A modifier belongs to one control: `email`, `unique`, `placeholder`, and
/// `multiline` to text; `options`, `options_with_labels`, `relationship`, and
/// `searchable` to choice. Calling one on another control is a declaration
/// bug and panics, naming the field.
pub struct Field {
    name: String,
    label: String,
    required: bool,
    /// Whether the column stores NULL for an empty submission, which a unique
    /// index admits any number of times.
    nullable: bool,
    /// The email and scalar-parse rules, with their messages.
    rules: Rules,
    control: Control,
}

/// The control a [`Field`] renders, with what only that control declares.
pub(crate) enum Control {
    /// A one-line `<input>`, or a `<textarea>` when `rows` is set.
    Text(TextControl),
    /// A `<select>` over static options or a relationship's rows.
    Choice(ChoiceControl),
    /// A file input storing the uploaded file's path.
    File,
}

impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The probe and the loaders are closures with no useful Debug;
        // everything a reader needs is the field's identity and its control.
        let control = match &self.control {
            Control::Text(_) => "text",
            Control::Choice(_) => "choice",
            Control::File => "file",
        };
        f.debug_struct("Field")
            .field("name", &self.name)
            .field("label", &self.label)
            .field("required", &self.required)
            .field("control", &control)
            .finish()
    }
}

impl Field {
    /// A text field over a column of any [`FormScalar`] type: `String`, a
    /// [`TypedValue`](crate::schema::TypedValue) type, or an `Option` of one.
    ///
    /// The control renders the stored value, and a typed column adds its
    /// spelling rule:
    ///
    /// - a submission the type refuses is an inline field error naming the input (`` `2024-13-01`
    ///   is not a valid timestamp ``);
    /// - what is stored is the type's own spelling of the parsed value, so a value re-submitted
    ///   unchanged is written back in the shape it was read.
    ///
    /// The control's `type` is the type's `INPUT_TYPE`: `datetime-local` for a
    /// `jiff::Timestamp`, whose stored instant renders in UTC and whose
    /// submission is read back as UTC, and `text` otherwise (`email` overrides
    /// it). `unique` defaults from the column's unique index (GH #183).
    ///
    /// `T` is [`IntoExpr`](toasty::stmt::IntoExpr) of itself so the unique
    /// probe compares the parsed value through the lens rather than its text.
    pub fn text<M, T>(lens: impl Into<ResolvedLens<M, T>>) -> Self
    where
        M: toasty::schema::Model,
        T: FormScalar + toasty::stmt::IntoExpr<T> + 'static,
    {
        let lens = lens.into();
        let control = TextControl::new::<M, T>(lens.path, lens.unique);
        Self {
            name: lens.name,
            label: lens.label,
            required: !lens.nullable,
            nullable: lens.nullable,
            rules: Rules::new().scalar::<T>(),
            control: Control::Text(control),
        }
    }

    /// The text field `#[derive(EmbeddedForm)]` renders for a leaf: bound
    /// through the request's app schema, and bounded by `FormScalar` alone so
    /// a leaf of another type fails at the derive's `FormScalar` assertion.
    #[doc(hidden)]
    pub fn embedded_leaf<M, T>(cx: &Cx, path: toasty::stmt::Path<M, T>) -> Self
    where
        M: toasty::schema::Model,
        T: FormScalar,
    {
        let lens = ResolvedLens::new(cx, path);
        Self {
            name: lens.name,
            label: lens.label,
            required: !lens.nullable,
            nullable: lens.nullable,
            rules: Rules::new().scalar::<T>(),
            control: Control::Text(TextControl::leaf::<T>()),
        }
    }

    /// A choice field over a column of any type, often a foreign key
    /// (`Post::fields().author_id()`).
    ///
    /// A bare choice validates presence only; its options come from
    /// [`options`](Self::options), [`options_with_labels`](Self::options_with_labels),
    /// or [`relationship`](Self::relationship), which also checks that a
    /// submitted key exists, tenant-aware.
    pub fn choice<M, T>(lens: impl Into<ResolvedLens<M, T>>) -> Self
    where
        M: toasty::schema::Model,
    {
        let lens = lens.into();
        Self {
            name: lens.name,
            label: lens.label,
            required: !lens.nullable,
            nullable: lens.nullable,
            rules: Rules::new(),
            control: Control::Choice(ChoiceControl::default()),
        }
    }

    /// A file field over a `String` column holding the uploaded file's
    /// **path**, never its bytes.
    ///
    /// A form holding one renders `enctype="multipart/form-data"`, and the
    /// POST parser extracts the file part; where the bytes go is the app's
    /// decision, expressed by the [`Uploader`](crate::Uploader) installed with
    /// [`Panel::uploads`](crate::Panel::uploads). With none, the sanitized
    /// basename is stored. The file input renders no `value` attribute, which
    /// browsers ignore for security, so an edit form shows the stored path and
    /// a `clear_<field>` checkbox beside an empty input, and the input is
    /// required only while nothing is stored.
    pub fn file<M>(lens: impl Into<ResolvedLens<M, String>>) -> Self
    where
        M: toasty::schema::Model,
    {
        let lens = lens.into();
        Self {
            name: lens.name,
            label: lens.label,
            required: !lens.nullable,
            nullable: lens.nullable,
            rules: Rules::new(),
            control: Control::File,
        }
    }

    /// The variant control of an embedded enum: a choice over its
    /// discriminant column, which the schema generates no lens for.
    ///
    /// Each option submits a variant's stored value and reads as its name. It
    /// is never required: an empty submit is "no variant named", which the
    /// value codec answers with its payload fallback, so refusing it would make
    /// that fallback unreachable.
    pub(crate) fn discriminant(name: String, variants: Vec<(String, String)>) -> Self {
        Self {
            label: capitalize(&name),
            name,
            required: false,
            nullable: true,
            rules: Rules::new(),
            control: Control::Choice(ChoiceControl {
                discriminant: true,
                options: variants,
                ..ChoiceControl::default()
            }),
        }
    }

    /// Override the label.
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// Refuse an empty submission.
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Accept an empty submission: for a nullable column, or for a column a
    /// record fn fills when the form leaves it empty. The browser-side
    /// `required` attribute drops too. A unique text field stays required
    /// ([`unique`](Self::unique)).
    pub fn optional(mut self) -> Self {
        self.required = false;
        self
    }

    /// Validate the value as an email address (text), and render
    /// `type="email"`.
    pub fn email(mut self) -> Self {
        self.text_mut("email");
        self.rules.set_email();
        self
    }

    /// Mark the field as backed by a unique index, which the app-side
    /// pre-check probes before the write (text).
    ///
    /// **Uniqueness implies presence on a non-nullable column**: an empty
    /// `String` stores `""`, which the index admits only once, so an empty
    /// submit is refused inline as `"<Label> is required"` instead of being
    /// written, and the probe never sees it. `.optional()` does not lift that
    /// rule, whichever order the two are called in (ADR-0010). A nullable
    /// column (`Option<T>`) stores NULL for an empty submit, which the index
    /// admits any number of times, so it stays optional when declared so.
    pub fn unique(mut self) -> Self {
        self.text_mut("unique").unique = true;
        self
    }

    /// The control's placeholder text (text).
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.text_mut("placeholder").placeholder = Some(placeholder.into());
        self
    }

    /// Render a `<textarea>` of `rows` lines rather than a one-line input
    /// (text), for a column holding prose.
    pub fn multiline(mut self, rows: u32) -> Self {
        self.text_mut("multiline").rows = Some(rows);
        self
    }

    /// Static options whose value is their label (choice).
    pub fn options(self, options: Vec<String>) -> Self {
        self.options_with_labels(options.into_iter().map(|s| (s.clone(), s)).collect())
    }

    /// Static options as `(value, label)` pairs (choice).
    pub fn options_with_labels(mut self, pairs: Vec<(String, String)>) -> Self {
        self.choice_mut("options").options = pairs;
        self
    }

    /// Filter the options as the user types (choice).
    ///
    /// Renders a filter input and a suggestion listbox above the select.
    /// Typing narrows the list by label substring for a bounded set, and a pick
    /// writes the chosen option onto the select, which stays the form control.
    /// Past the option cap, a relationship fetches
    /// `GET {parent_list_url}/options?field=&q=` (debounced, in-flight
    /// requests aborted, selection preserved) and re-renders the list from the
    /// answer, searching the related table's `searchable()` columns; a
    /// non-searchable relationship keeps the cap error.
    ///
    /// Needs `assets/selects.js` (`tablo_ui::SELECTS_JS`), emitted by
    /// `Panel::render_document` on every document with shell assets
    /// (ADR-0014); without it the input is inert and the plain select keeps
    /// working.
    pub fn searchable(mut self) -> Self {
        self.choice_mut("searchable").searchable = true;
        self
    }

    /// Load the options from a related source's tenant-scoped query (choice).
    ///
    /// `R` is any [`OptionSource`] — every `Resource` is one. The first
    /// argument is the related resource's `query` fn (`AuthorResource::query`),
    /// a type-inference witness only: the loader calls the source's scoped
    /// query, so the tenant gate and the derived tenant filter apply. The
    /// second projects a record to the model's **primary key**, whose
    /// `Display` is the `<option value>`; the third maps it to its label. A
    /// projection to anything but the key type fails to compile, and an edit
    /// form hydrates the foreign key with the same string.
    ///
    /// Policy-checked: the related source must allow `can_view_any` and, when
    /// it declares `requires_tenant`, have a resolved tenant; each loaded row
    /// is filtered through `can_view`. A denial fails the load closed — no
    /// options and not the stored value, `{label} is not available` on GET,
    /// and a submit that carries a value fails with that message.
    ///
    /// Bounded and memoized: at most one row past `MAX_RELATIONSHIP_OPTIONS`
    /// loads per `(request, tenant)`. Past the cap a searchable field degrades
    /// to type-to-search with a targeted existence check, and a non-searchable
    /// one reports `could not load options, retry`.
    pub fn relationship<R>(
        mut self,
        query: fn(&Cx) -> toasty::stmt::Query<toasty::stmt::List<R::Model>>,
        value: impl Fn(&R::Model) -> RelatedPrimaryKey<R> + Send + Sync + 'static,
        label: impl Fn(&R::Model) -> String + Send + Sync + 'static,
    ) -> Self
    where
        R: OptionSource + 'static,
        RelatedPrimaryKey<R>: std::fmt::Display,
    {
        let _ = query;
        self.choice_mut("relationship").relationship =
            Some(choice::Relationship::new::<R>(value, label));
        self
    }

    /// The key the control posts.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The label the control renders.
    pub(crate) fn label_str(&self) -> &str {
        &self.label
    }

    /// The choice control, when this is a choice field.
    pub(crate) fn as_choice(&self) -> Option<&ChoiceControl> {
        match &self.control {
            Control::Choice(choice) => Some(choice),
            _ => None,
        }
    }

    /// The equality expression the app-side unique check probes with: the
    /// submission parsed into the field's type, compared through its lens.
    /// `None` for a submission the type refuses, and for a field that is not
    /// text.
    pub(crate) fn eq_filter(&self, value: &str) -> Option<toasty::stmt::Expr<bool>> {
        match &self.control {
            Control::Text(text) => text.eq_filter(value),
            _ => None,
        }
    }

    /// Whether this is a file field.
    pub(crate) fn is_file(&self) -> bool {
        matches!(self.control, Control::File)
    }

    /// Whether this is a text field marked [`unique`](Self::unique).
    pub(crate) fn is_unique(&self) -> bool {
        matches!(&self.control, Control::Text(text) if text.unique)
    }

    /// Whether an empty submit fails validation and the control renders as
    /// required: `required`, or a unique text field over a non-nullable
    /// column (GH #189). `validate` and the render read it, so the rule and
    /// the marker cannot disagree.
    pub(crate) fn is_required(&self) -> bool {
        self.required || (self.is_unique() && !self.nullable)
    }

    /// Validate a raw submitted value against the field's rules.
    pub(crate) fn validate(&self, value: &str) -> Vec<String> {
        self.rules.validate(&self.label, self.is_required(), value)
    }

    /// The stored spelling of a submission the caller has already validated:
    /// the scalar parse's spelling for a text field, the trimmed submission
    /// otherwise.
    ///
    /// A choice stores its trimmed submission because its presence rule and
    /// its option-existence check both read the trimmed value, so the trimmed
    /// value is the one that passed.
    pub(crate) fn normalize(&self, value: &str) -> Result<String, String> {
        self.rules.normalize(value)
    }

    /// Existence of a submitted choice among its options: a static option, or
    /// a relationship row the user may view. Empty for any other field.
    pub(crate) async fn validate_exists(&self, cx: &Cx, value: &str) -> Vec<String> {
        match &self.control {
            Control::Choice(choice) => choice.validate_exists(cx, &self.label, value).await,
            _ => Vec::new(),
        }
    }

    /// Render the field: its control in `Mode::Form`, its stored value in
    /// `Mode::View`.
    ///
    /// A view field whose key the values do not carry renders `(missing)` and
    /// fails a `debug_assert!`: neither the resource's `view_values` nor its
    /// record form supplies the key, so the page would otherwise show a blank
    /// that reads as an empty value. It is the contract the list columns keep
    /// for an unloaded relation (ADR-0011).
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        value: Option<&str>,
        errors: &[String],
        mode: Mode,
    ) -> Result<BoxView<'a>> {
        if mode == Mode::View && value.is_none() {
            debug_assert!(
                false,
                "view field `{}` has no value: neither `view_values` nor the record form's \
                 `hydrate` supplies its key",
                self.name
            );
            return render_value(cx, &self.label, Some("(missing)"), ValueKind::Prose);
        }
        match &self.control {
            Control::Text(text) => self.render_text(text, cx, value, errors, mode),
            Control::Choice(choice) => {
                Box::pin(self.render_choice(choice, cx, value, errors, mode)).await
            }
            Control::File => self.render_file(cx, value, errors, mode),
        }
    }

    /// The text control, or a panic naming the modifier that needs one.
    fn text_mut(&mut self, modifier: &str) -> &mut TextControl {
        match &mut self.control {
            Control::Text(text) => text,
            _ => panic!(
                "`.{modifier}()` applies to a text field, and `{}` is not one",
                self.name
            ),
        }
    }

    /// The choice control, or a panic naming the modifier that needs one.
    fn choice_mut(&mut self, modifier: &str) -> &mut ChoiceControl {
        match &mut self.control {
            Control::Choice(choice) => choice,
            _ => panic!(
                "`.{modifier}()` applies to a choice field, and `{}` is not one",
                self.name
            ),
        }
    }
}

/// How a read-only value is presented.
///
/// Two shapes, because the difference is content, not styling: prose wraps at
/// spaces, and an identifier (a stored path, an address) has no spaces to break
/// at, so it breaks anywhere and sets in mono.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueKind {
    /// Wrapping text: a title, a body, a description.
    Prose,
    /// A path, a key, an address — no spaces to break at.
    Machine,
}

/// The read-only half of a field: the label with the record's stored
/// value under it, no control and no validation slot.
///
/// Every control renders its view through this, so a detail page reads
/// uniformly. The label is the same `field_label` the form uses, inside the
/// same `field` family, so a field is recognisable across the two pages.
///
/// An empty value renders empty: a `String` column stores `""` and a NULL
/// hydrates as `""`, so the page cannot tell "no value" from an empty one.
fn render_value<'a>(
    cx: &'a Cx,
    label: &str,
    value: Option<&str>,
    kind: ValueKind,
) -> Result<BoxView<'a>> {
    let text = value.unwrap_or_default().to_string();
    let value_class = match kind {
        ValueKind::Prose => "text-sm break-words whitespace-pre-wrap",
        ValueKind::Machine => "text-sm font-mono break-all whitespace-pre-wrap",
    };
    let value = view! { cx => <div class=(value_class)>(text)</div> }.boxed();
    render_value_view(cx, label, value)
}

/// The field chrome `render_value` puts around a rendered value, for
/// a field whose read-only value is not a plain string.
///
/// A file field renders its stored path as a link and supplies that view
/// here, so the label, the `field` family and the `ac-field` marker stay the
/// ones every other read-only field renders through.
fn render_value_view<'a>(cx: &'a Cx, label: &str, value: BoxView<'a>) -> Result<BoxView<'a>> {
    let label = label.to_string();
    Ok(view! {
        cx =>
        ui_field(
            attrs: attributes! { class="ac-field" },
            ui_field_content(
                ui_field_title((label))
                (value)
            )
        )
    }
    .boxed())
}

/// The validation state a form control renders: the error id its
/// `aria-describedby` points at, the message its error slot shows, and whether
/// the field is invalid.
///
/// A field is invalid when it carries an error or when it has a `fallback`
/// message of its own — the relationship denial a choice surfaces on GET,
/// which has no `errors` entry yet.
pub(crate) struct FieldChrome {
    name: String,
    error_id: String,
    error_text: String,
    has_error: bool,
}

impl FieldChrome {
    pub(crate) fn new(name: &str, errors: &[String], fallback: Option<String>) -> Self {
        let incoming = errors.first().cloned().unwrap_or_default();
        let has_error = !errors.is_empty() || fallback.is_some();
        let error_text = if incoming.is_empty() {
            fallback.unwrap_or_default()
        } else {
            incoming
        };
        Self {
            name: name.to_string(),
            error_id: format!("{name}-error"),
            error_text,
            has_error,
        }
    }

    /// The control's `aria-invalid`: `"true"` also colors the field's label.
    pub(crate) fn aria_invalid(&self) -> &'static str {
        if self.has_error { "true" } else { "false" }
    }

    /// The control's `aria-describedby`, pointing at the error slot while the
    /// field is invalid. Owned because a rendered view outlives this value.
    pub(crate) fn described_by(&self) -> Option<String> {
        self.has_error.then(|| self.error_id.clone())
    }
}

/// The chrome every form control renders: the `field` wrapper carrying
/// `ac-field` / `ac-field--error`, the label with the required marker, the
/// control, and the error slot.
///
/// `attributes` carries the extra wrapper attributes a control needs — the
/// choice option and filter hooks.
pub(crate) fn render_field<'a>(
    cx: &'a Cx,
    chrome: &FieldChrome,
    label: &str,
    required: bool,
    attributes: Attributes,
    control: BoxView<'a>,
) -> Result<BoxView<'a>> {
    let name = chrome.name.clone();
    let label_text = label.to_string();
    let has_error = chrome.has_error;
    let error_id = chrome.error_id.clone();
    let error_text = chrome.error_text.clone();
    let field_class = if has_error {
        "ac-field ac-field--error"
    } else {
        "ac-field"
    };
    Ok(view! {
        cx =>
        ui_field(
            attrs: attributes! {
                class=(field_class)
                data-invalid=(has_error.then_some("true"))
                (attributes)
            },
            ui_field_label(
                attrs: attributes! { for=(name) },
                (label_text)
                if required {
                    <span class="text-destructive" aria-hidden="true">"*"</span>
                }
            )
            (control)
            if has_error {
                ui_field_error(
                    attrs: attributes! { id=(error_id) class="ac-error" aria-live="polite" },
                    (error_text)
                )
            }
        )
    }
    .boxed())
}

#[cfg(test)]
mod test_support {
    pub(super) use crate::test_support::cx;
    #[derive(Debug, toasty::Model)]
    pub(super) struct DummyUser {
        #[key]
        #[auto]
        id: uuid::Uuid,
        name: String,
        #[unique]
        email: String,
    }

    /// The whole opening tag carrying `needle` — how a test asserts on an
    /// element whose attributes render in no guaranteed order (topcoat#122)
    /// without depending on a marker attribute nothing consumes.
    pub(super) fn tag_with<'h>(html: &'h str, needle: &str) -> &'h str {
        let at = html
            .find(needle)
            .unwrap_or_else(|| panic!("no {needle} in {html}"));
        opening_tag_at(html, html[..at].rfind('<').expect("its opening tag"))
    }

    /// The opening tag that starts at `start`, sliced up to the `>` closing it.
    ///
    /// `Attributes` renders in no guaranteed order (topcoat#122), so a test
    /// locates a tag by whichever attribute it can and asserts on the whole
    /// tag. Quoting is honoured, so a `>` inside an attribute value (Tailwind
    /// selectors carry them) does not end the slice.
    pub(super) fn opening_tag_at(html: &str, start: usize) -> &str {
        let mut quoted = false;
        for (offset, byte) in html.as_bytes()[start..].iter().enumerate() {
            match byte {
                b'"' => quoted = !quoted,
                b'>' if !quoted => return &html[start..start + offset],
                _ => {}
            }
        }
        panic!("unterminated tag at byte {start} in {html}");
    }

    /// The attributes of the opening tag carrying `needle`, sorted — quoting is
    /// honoured, so a Tailwind class value stays one token.
    ///
    /// `Attributes` renders in no guaranteed order (topcoat#122), so two
    /// renders of the same markup compare as sets of `name="value"` tokens.
    pub(super) fn attributes_of(html: &str, needle: &str) -> Vec<String> {
        let mut quoted = false;
        let mut attrs: Vec<String> = Vec::new();
        let mut current = String::new();
        for ch in tag_with(html, needle).chars() {
            match ch {
                '"' => {
                    quoted = !quoted;
                    current.push(ch);
                }
                ch if ch.is_whitespace() && !quoted => {
                    if !current.is_empty() {
                        attrs.push(std::mem::take(&mut current));
                    }
                }
                ch => current.push(ch),
            }
        }
        if !current.is_empty() {
            attrs.push(current);
        }
        attrs.remove(0); // the tag name
        attrs.sort();
        attrs
    }

    /// A nullable FK, for the optional-by-default choice case.
    #[derive(Debug, toasty::Model)]
    pub(super) struct NullableRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        parent_id: Option<uuid::Uuid>,
    }

    /// A non-nullable foreign key, for the required-by-default FK choice.
    #[derive(Debug, toasty::Model)]
    pub(super) struct FkRef {
        #[key]
        #[auto]
        id: uuid::Uuid,
        author_id: uuid::Uuid,
    }
}

#[cfg(test)]
mod tests;
