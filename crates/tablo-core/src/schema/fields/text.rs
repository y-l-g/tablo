//! The text control: a one-line `<input>`, or a `<textarea>` with `rows`.

use jiff::tz::TimeZone;
use tablo_ui::{input as ui_input, textarea as ui_textarea};
use topcoat::{Result, context::Cx, icon::icon, view::*};

pub(crate) use self::rules::Pattern;
use self::rules::{Order, Rules, order};
use super::{ControlKind, Field, render_field};
use crate::{
    FieldRuleFault,
    schema::comparison::Comparison,
    values::{FormScalar, FormValue},
};

/// The equality expression a text field's unique probe binds.
type EqProbe = std::sync::Arc<dyn Fn(&str) -> Option<toasty::stmt::Expr<bool>> + Send + Sync>;

/// What a text control's value is beyond its type: it picks the `type` attribute, and the server
/// checks an email or a URL.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Kind {
    /// Whatever the field's type parses, in the type's own input.
    #[default]
    Typed,
    /// An email address: `type="email"`.
    Email,
    /// An absolute `http` or `https` URL: `type="url"`.
    Url,
    /// A telephone number, in no checked format: `type="tel"`.
    Tel,
}

/// The text control's input type, placeholder, rows, kind and password rules, declared rules,
/// and unique probe.
pub(crate) struct TextControl {
    input_type: &'static str,
    step: Option<&'static str>,
    /// The control's `value` for a stored spelling, in the request's time zone.
    input_value: fn(&str, &TimeZone) -> String,
    /// The form spelling of a posted value, read in the request's time zone.
    form_value: fn(&str, &TimeZone) -> String,
    pub(super) rows: Option<u32>,
    pub(super) placeholder: Option<crate::Text>,
    /// The text before and after a one-line input, such as a currency or a unit.
    pub(super) prefix: Option<crate::Text>,
    pub(super) suffix: Option<crate::Text>,
    /// The values a one-line input suggests through a `<datalist>`, offering no other.
    pub(super) suggestions: Vec<String>,
    pub(super) kind: Kind,
    pub(super) password: bool,
    /// The password the user signs in with today, which a password manager fills, rather than
    /// a new one it would offer to generate.
    pub(super) current_password: bool,
    pub(super) unique: bool,
    /// Each unique index covering the column, as the names of its other fields.
    unique_with: Vec<Vec<String>>,
    pub(super) rules: Rules,
    probe: Option<EqProbe>,
    /// The stored spelling of a submission, or `None` when the type refuses it.
    spell: fn(&str) -> Option<String>,
    /// How the field's type orders two submissions, for its bounds.
    order: Order,
    /// The type its comparisons order by: [`FormScalar::compared_as`].
    compared_as: &'static str,
    pub(super) comparisons: Vec<Comparison>,
}

/// The stored spelling of a `T` submission.
fn spell<T: FormScalar>(value: &str) -> Option<String> {
    T::parse_form(value.trim())
        .ok()
        .map(|parsed| parsed.to_form())
}

impl TextControl {
    /// Parses a submission into `T` and compares through the lens, so `01` and `1` check as the
    /// stored integer.
    pub(super) fn new<M, T>(
        path: toasty::stmt::Path<M, T>,
        unique: bool,
        unique_with: Vec<Vec<String>>,
    ) -> Self
    where
        T: FormScalar + toasty::stmt::IntoExpr<T> + 'static,
    {
        let path = crate::toasty_compat::model::ModelPath::of(&path);
        let probe: EqProbe = std::sync::Arc::new(move |value: &str| {
            let parsed = T::parse_form(value.trim()).ok()?;
            Some(path.eq(parsed))
        });
        Self {
            input_type: T::INPUT_TYPE,
            step: T::STEP,
            input_value: T::input_value,
            form_value: T::form_value,
            rows: None,
            placeholder: None,
            prefix: None,
            suffix: None,
            suggestions: Vec::new(),
            kind: Kind::Typed,
            password: false,
            current_password: false,
            unique,
            unique_with,
            rules: Rules::default(),
            probe: Some(probe),
            spell: spell::<T>,
            order: order::<T>,
            compared_as: T::compared_as(),
            comparisons: Vec::new(),
        }
    }

    /// The control of a derived embedded leaf, with no unique probe.
    pub(super) fn leaf<T: FormScalar>() -> Self {
        Self {
            input_type: T::INPUT_TYPE,
            step: T::STEP,
            input_value: T::input_value,
            form_value: T::form_value,
            rows: None,
            placeholder: None,
            prefix: None,
            suffix: None,
            suggestions: Vec::new(),
            kind: Kind::Typed,
            password: false,
            current_password: false,
            unique: false,
            unique_with: Vec::new(),
            rules: Rules::default(),
            probe: None,
            spell: spell::<T>,
            order: order::<T>,
            compared_as: T::compared_as(),
            comparisons: Vec::new(),
        }
    }

    /// The control's `value` for the stored spelling `stored`, shown in `zone`.
    pub(crate) fn rendering(&self, stored: &str, zone: &TimeZone) -> String {
        (self.input_value)(stored, zone)
    }

    /// The form spelling of `posted`, what the control posted, read in `zone`.
    pub(crate) fn reading(&self, posted: &str, zone: &TimeZone) -> String {
        (self.form_value)(posted, zone)
    }

    /// The fields each unique index covering the column holds besides it: one empty list for a
    /// field marked unique with no index, which the declaration check refuses.
    pub(crate) fn unique_with(&self) -> Vec<&[String]> {
        if self.unique_with.is_empty() {
            return vec![&[]];
        }
        self.unique_with.iter().map(Vec::as_slice).collect()
    }

    /// The `type` of the control's `<input>`: `text` for a `String`.
    pub(crate) fn input_type(&self) -> &'static str {
        self.input_type
    }

    /// Whether the field's type parses `value`.
    pub(crate) fn parses(&self, value: &str) -> bool {
        (self.spell)(value).is_some()
    }

    /// Whether two submissions spell the same stored value: `01` and `1` do for an integer.
    pub(crate) fn same_value(&self, a: &str, b: &str) -> bool {
        matches!(((self.spell)(a), (self.spell)(b)), (Some(a), Some(b)) if a == b)
    }

    /// The sentence refusing `value`, a trimmed, non-empty submission, for the first declared
    /// rule it breaks. `None` for a value the field's type does not parse: its parse refuses it,
    /// in the type's own words. A bound reads as the control shows it in `zone`.
    pub(crate) fn refusal(
        &self,
        label: &crate::Text,
        value: &str,
        zone: &TimeZone,
    ) -> Option<crate::Text> {
        if !self.parses(value) {
            return None;
        }
        self.rules.refusal(label, value, self.order, |bound| {
            Some(self.rendering(bound, zone))
                .filter(|shown| !shown.is_empty())
                .unwrap_or_else(|| bound.to_string())
        })
    }

    /// How the field's type orders two submissions, `None` when it parses neither or has no order.
    pub(crate) fn order(&self, a: &str, b: &str) -> Option<std::cmp::Ordering> {
        (self.order)(a, b)
    }

    /// The type the field's comparisons order by, which a compared field shares.
    pub(crate) fn compared_as(&self) -> &'static str {
        self.compared_as
    }

    /// The field's comparisons with other fields of its form.
    pub(crate) fn comparisons(&self) -> &[Comparison] {
        &self.comparisons
    }

    /// What is wrong with the declared rules.
    pub(crate) fn rule_faults(&self) -> Vec<FieldRuleFault> {
        self.rules.faults(self.order)
    }

    /// Whether the control is a one-line input showing what it holds: no text area, no password.
    pub(crate) fn is_one_line(&self) -> bool {
        self.rows.is_none() && !self.password
    }

    /// The equality expression the app-side unique check probes with.
    pub(crate) fn eq_filter(&self, value: &str) -> Option<toasty::stmt::Expr<bool>> {
        self.probe.as_ref().and_then(|probe| probe(value))
    }
}

impl Field {
    pub(crate) fn as_text(&self) -> Option<&TextControl> {
        match &self.control {
            ControlKind::Text(text) => Some(text),
            _ => None,
        }
    }

    /// Probes uniqueness with the submission parsed through the lens.
    pub(crate) fn eq_filter(&self, value: &str) -> Option<toasty::stmt::Expr<bool>> {
        self.as_text().and_then(|text| text.eq_filter(value))
    }

    /// Compares under `prefix`, as a repeater row posts its sibling fields.
    pub(super) fn prefix_comparisons(&mut self, prefix: &str) {
        if let ControlKind::Text(text) = &mut self.control {
            for comparison in &mut text.comparisons {
                comparison.other.insert_str(0, prefix);
            }
        }
    }

    /// The form spelling of `posted`, the last value the field's control posted, read in `zone`:
    /// a text control reads it through its type, as a timestamp's reads a wall-clock time, a rich
    /// text control keeps what its toolbar produces, and any other control keeps it as posted.
    pub(crate) fn read_posted(&self, posted: &str, zone: &TimeZone) -> String {
        match &self.control {
            ControlKind::Text(text) => text.reading(posted, zone),
            #[cfg(feature = "rich-text")]
            ControlKind::RichText(rich) => rich.clean(posted),
            _ => posted.to_string(),
        }
    }

    /// The stored value `stored` when a text control posts `value` as it rendered `stored` in
    /// `zone`, or `None` when the user changed it. A control coarser than its column, as a
    /// timestamp's is to the minute, renders a rounded value: posting it back untouched must not
    /// round the column. `value` is already read in `zone`, so the rendering is read back the same
    /// way before the two compare. A repeater pairs its posted rows with its stored rows by
    /// position and keeps each row's unedited values the same way.
    pub(crate) fn keep_unedited(
        &self,
        value: &FormValue,
        stored: &FormValue,
        zone: &TimeZone,
    ) -> Option<FormValue> {
        let text = match &self.control {
            ControlKind::Text(text) => text,
            ControlKind::Repeater(repeater) => {
                let (Some(rows), Some(held)) = (value.as_rows(), stored.as_rows()) else {
                    return None;
                };
                let item = repeater.item_schema();
                let mut rows = rows.to_vec();
                for (row, held) in rows.iter_mut().zip(held) {
                    item.keep_unedited(row, held, zone);
                }
                return Some(FormValue::Rows(rows));
            }
            _ => return None,
        };
        let (Some(value), Some(held)) = (value.as_text(), stored.as_text()) else {
            return None;
        };
        let rendered = text.reading(&text.rendering(held, zone), zone);
        (value != held && value.trim() == rendered).then(|| stored.clone())
    }

    /// Renders a text field's control.
    pub(super) fn render_text<'a>(
        &self,
        text: &TextControl,
        cx: &'a Cx,
        value: Option<&str>,
        error: Option<&crate::values::FieldError>,
        id: String,
        disabled: bool,
    ) -> Result<BoxView<'a>> {
        let name = self.name().to_string();
        let required = self.is_required() && !disabled;
        let placeholder = text.placeholder.clone();
        let chrome = self.chrome(cx, id.clone(), error, None);
        let aria_invalid = chrome.aria_invalid();
        let described_by = chrome.described_by();
        let rules = &text.rules;
        let (min_length, max_length) = (rules.min_length, rules.max_length);
        // A password renders the masked input even when declared multiline: a `<textarea>` would
        // show it and echo it back into a refused form.
        let control = if let Some(rows) = text.rows.filter(|_| !text.password) {
            // A `<textarea>` takes its initial value from content, not a `value` attribute.
            let value_owned = value.unwrap_or("").to_string();
            view! {
                cx =>
                ui_textarea(
                    attrs: attributes! {
                        id=(id.clone())
                        name=(name.clone())
                        placeholder=(placeholder.clone())
                        rows=(rows)
                        minlength=(min_length)
                        maxlength=(max_length)
                        required=(required)
                        disabled=(disabled)
                        aria-required=(required.then_some("true"))
                        aria-invalid=(aria_invalid)
                        aria-describedby=(described_by)
                    },
                    (value_owned)
                )
            }
            .boxed()
        } else {
            let input_type = match text.kind {
                _ if text.password => "password",
                Kind::Email => "email",
                Kind::Url => "url",
                Kind::Tel => "tel",
                Kind::Typed => text.input_type,
            };
            // A password never travels back to the browser: a refused form renders it empty.
            let value = value.filter(|_| !text.password);
            let autocomplete = text.password.then_some(if text.current_password {
                "current-password"
            } else {
                "new-password"
            });
            let zone = crate::time_zone(cx);
            let value_owned = value.map(|value| text.rendering(value, &zone));
            let step = text.step.filter(|_| input_type == text.input_type);
            let min = rules.min.as_deref().map(|min| text.rendering(min, &zone));
            let max = rules.max.as_deref().map(|max| text.rendering(max, &zone));
            let pattern = rules.pattern.as_ref().map(|pattern| pattern.source.clone());
            let controls = id.clone();
            // A password suggests nothing: a datalist would put the values in the page.
            let list_id = (!text.suggestions.is_empty() && !text.password)
                .then(|| format!("{id}-suggestions"));
            let (prefix, suffix) = (text.prefix.clone(), text.suffix.clone());
            let prefix_id = prefix.as_ref().map(|_| format!("{id}-prefix"));
            let suffix_id = suffix.as_ref().map(|_| format!("{id}-suffix"));
            // A screen reader reads an affix with the control, as a sighted user sees it.
            let described_by = [prefix_id.clone(), suffix_id.clone(), described_by]
                .into_iter()
                .flatten()
                .reduce(|ids, id| format!("{ids} {id}"));
            // The input's own class sets its rounding, which `class!` cannot drop: the side an
            // affix joins overrides it.
            let joined = match (prefix.is_some(), suffix.is_some()) {
                (true, true) => Some("rounded-none!"),
                (true, false) => Some("rounded-l-none!"),
                (false, true) => Some("rounded-r-none!"),
                (false, false) => None,
            };
            let list = list_id.clone();
            let input = view! {
                cx =>
                ui_input(
                    attrs: attributes! {
                        id=(id.clone())
                        type=(input_type)
                        step=(step)
                        name=(name.clone())
                        value=(value_owned.clone())
                        placeholder=(placeholder.clone())
                        minlength=(min_length)
                        maxlength=(max_length)
                        min=(min)
                        max=(max)
                        pattern=(pattern)
                        list=(list)
                        class=(joined)
                        autocomplete=(autocomplete)
                        required=(required)
                        disabled=(disabled)
                        aria-required=(required.then_some("true"))
                        aria-invalid=(aria_invalid)
                        aria-describedby=(described_by)
                    }
                )
            }
            .boxed();
            let input = if text.password {
                password_reveal(cx, controls, input)
            } else {
                input
            };
            let suggestions = list_id.map(|list_id| (list_id, text.suggestions.clone()));
            affixed(
                cx,
                input,
                prefix.zip(prefix_id),
                suffix.zip(suffix_id),
                suggestions,
            )
        };
        render_field(
            cx,
            &chrome,
            &self.label(),
            required,
            attributes! {},
            control,
        )
    }
}

/// `input` between its `prefix` and `suffix`, each a text and its id, followed by the datalist
/// `suggestions` names, its id and values; `input` alone when it has none of them.
fn affixed<'a>(
    cx: &'a Cx,
    input: BoxView<'a>,
    prefix: Option<(crate::Text, String)>,
    suffix: Option<(crate::Text, String)>,
    suggestions: Option<(String, Vec<String>)>,
) -> BoxView<'a> {
    const AFFIX: &str = "inline-flex shrink-0 items-center border border-border bg-muted px-3 \
         text-sm text-muted-foreground";
    let input = if prefix.is_none() && suffix.is_none() {
        input
    } else {
        view! {
            cx =>
            <div class="flex items-stretch">
                if let Some((text, id)) = prefix {
                    <span id=(id) class=(format!("{AFFIX} rounded-l-lg border-r-0"))>
                        (text)
                    </span>
                }
                <div class="min-w-0 flex-1">(input)</div>
                if let Some((text, id)) = suffix {
                    <span id=(id) class=(format!("{AFFIX} rounded-r-lg border-l-0"))>
                        (text)
                    </span>
                }
            </div>
        }
        .boxed()
    };
    let Some((id, values)) = suggestions else {
        return input;
    };
    view! {
        cx =>
        (input)
        <datalist id=(id)>
            for value in values {
                <option value=(value)></option>
            }
        </datalist>
    }
    .boxed()
}

/// `input`, the password input `id` names, with the button beside it that shows or masks the
/// value: `assets/password.js` toggles it.
pub(crate) fn password_reveal<'a>(cx: &'a Cx, id: String, input: BoxView<'a>) -> BoxView<'a> {
    view! {
        cx =>
        <div class="relative">
            (input)
            <button
                type="button"
                class="absolute inset-y-0 right-0 flex w-9 items-center justify-center rounded-r-md text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring [&>svg]:size-4"
                aria-controls=(id)
                aria-label=(crate::t!(cx, "tablo-password-show"))
                aria-pressed="false"
                data-password-toggle=""
                data-show-label=(crate::t!(cx, "tablo-password-show"))
                data-hide-label=(crate::t!(cx, "tablo-password-hide"))
            >
                icon(data: tablo_ui::icons::EYE)
            </button>
        </div>
    }
    .boxed()
}

mod rules;
#[cfg(test)]
mod tests;
