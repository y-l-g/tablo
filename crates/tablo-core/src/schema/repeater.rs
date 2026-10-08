//! Repeaters: a list of values stored in one `#[document]` column, edited one row per item.
//!
//! Each row posts its item's keys under `{key}.{row}.`, and the repeater's own key posts the rows
//! in the order they show, so a row the browser adds, removes or moves needs no other key. A
//! submission folds them into the repeater's key, as a JSON array of each item's keys, before
//! anything reads it: the edit's stored value, a hidden repeater and the parse then see one key
//! like any other field's.
//!
//! The browser adds a row by copying the blank row the repeater renders in a `<template>`, its
//! keys carrying [`ROW`] for the row's number (`assets/repeaters.js`).

use std::collections::{HashMap, HashSet};

use tablo_ui::{
    ButtonSize, ButtonVariant, FieldLegendVariant, button, field_error as ui_field_error,
    field_legend, field_set,
};
use topcoat::{Result, context::Cx, view::*};

use super::{Schema, fields::Field, tree::Source};
use crate::form::{FieldError, FieldErrors};

/// One item of a repeater: the typed value each row parses into, stored as a `toasty::Embed`
/// struct in a `#[document]` list.
///
/// Derive it with [`RepeaterItem`](derive@crate::RepeaterItem). Its fields post their own names,
/// as an [`ActionInput`](trait@crate::ActionInput)'s do, under the row's prefix.
pub trait RepeaterItem: Sized + Send + Sync + 'static {
    /// One row's controls, each posting its field's name.
    fn schema() -> Schema;

    /// Parse one row's submission of [`schema`](Self::schema)'s keys.
    ///
    /// # Errors
    ///
    /// Every key that failed, each once.
    fn parse(
        cx: &Cx,
        values: &HashMap<String, String>,
    ) -> std::result::Result<Self, Vec<FieldError>>;

    /// Write the item's keys as its row posts them.
    fn write(&self, out: &mut HashMap<String, String>);
}

/// What the blank row's keys carry for the number the browser gives a row it adds.
pub(crate) const ROW: &str = "__row__";

/// A repeater's control: its item's controls, one row each.
pub(crate) struct RepeaterControl {
    item: fn() -> Schema,
    /// The add button's label, over "Add item".
    pub(crate) add_label: Option<String>,
}

impl RepeaterControl {
    pub(crate) fn new<T: RepeaterItem>() -> Self {
        Self {
            item: T::schema,
            add_label: None,
        }
    }

    /// Folds the rows posted under `key` into `key`, in the order it lists them. A row's key
    /// that its item does not declare, or that a row it does not list posts, stays where it is
    /// for the unknown-key check to refuse. A submission that does not post `key` folds nothing,
    /// so an edit keeps the stored rows.
    ///
    /// # Errors
    ///
    /// `key` lists something other than row numbers, or a row twice.
    pub(crate) fn fold(
        &self,
        key: &str,
        values: &mut HashMap<String, String>,
    ) -> std::result::Result<(), String> {
        let Some(order) = values.remove(key) else {
            return Ok(());
        };
        let leaves: Vec<String> = (self.item)()
            .fields()
            .map(|field| field.name().to_string())
            .collect();
        let mut seen = HashSet::new();
        let mut rows = Vec::new();
        for token in order.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            let row: usize = token
                .parse()
                .map_err(|_| format!("`{key}` lists `{token}`, which is not a row"))?;
            if !seen.insert(row) {
                return Err(format!("`{key}` lists row {row} twice"));
            }
            let item: HashMap<String, String> = leaves
                .iter()
                .filter_map(|leaf| {
                    let value = values.remove(&row_key(key, &row.to_string(), leaf))?;
                    Some((leaf.clone(), value))
                })
                .collect();
            rows.push(item);
        }
        values.insert(key.to_string(), encode(&rows));
        Ok(())
    }

    /// Renders the repeater `field` from `source`: a row per stored or posted item, and the blank
    /// row the add button copies.
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        field: &Field,
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let key = field.name().to_string();
        let items = source.value(&key).and_then(decode).unwrap_or_default();
        let mut rows = Vec::with_capacity(items.len());
        for (row, item) in items.iter().enumerate() {
            rows.push(
                self.render_row(cx, &key, &row.to_string(), item, source.errors(), source)
                    .await?,
            );
        }
        let blank = self
            .render_row(cx, &key, ROW, &HashMap::new(), &FieldErrors::new(), source)
            .await?;
        let order = (0..items.len())
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let next = items.len().to_string();
        let label = field.label_str().to_string();
        let add = self
            .add_label
            .clone()
            .unwrap_or_else(|| "Add item".to_string());
        let error = source.error_for(field);
        let legend_id = format!("{}-legend", source.id(&key));
        Ok(view! {
            cx =>
            field_set(
                attrs: attributes! {
                    class="ac-field gap-3"
                    data-repeater=(key.clone())
                    data-repeater-next=(next)
                },
                field_legend(
                    variant: FieldLegendVariant::Label,
                    attrs: attributes! { id=(legend_id.clone()) class="mb-0" },
                    (label)
                )
                <input type="hidden" name=(key) value=(order) data-repeater-order="">
                if let Some(error) = error {
                    ui_field_error(attrs: attributes! { class="ac-error" }, (error))
                }
                <ol
                    class="flex flex-col gap-3"
                    aria-labelledby=(legend_id)
                    data-repeater-rows=""
                >
                    for row in rows {
                        (row)
                    }
                </ol>
                <template data-repeater-blank="">(blank)</template>
                <div>
                    button(
                        variant: ButtonVariant::Outline,
                        size: ButtonSize::Sm,
                        attrs: attributes! { type="button" data-repeater-add="" },
                        (add)
                    )
                </div>
            )
        }
        .boxed())
    }

    /// Renders the row `row` of the repeater `key` holding `item`, its controls posting under the
    /// row's prefix, with the buttons that move and remove it.
    async fn render_row<'a>(
        &self,
        cx: &'a Cx,
        key: &str,
        row: &str,
        item: &HashMap<String, String>,
        errors: &FieldErrors,
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let prefix = row_key(key, row, "");
        let mut schema = (self.item)();
        for field in &mut schema.fields {
            field.prefix(&prefix);
        }
        let values: HashMap<String, String> = item
            .iter()
            .map(|(leaf, value)| (format!("{prefix}{leaf}"), value.clone()))
            .collect();
        let mut rendered = Source::form(&values, errors).scoped(source.scope());
        if let Some(options) = source.options() {
            rendered = rendered.options_at(options);
        }
        let controls = schema.render(cx, rendered).await?;
        let row = row.to_string();
        let action = |label: &'static str, data: Attributes| {
            view! {
                cx =>
                button(
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    attrs: attributes! { type="button" (data) },
                    (label)
                )
            }
        };
        Ok(view! {
            cx =>
            <li
                class="flex flex-col gap-4 rounded-lg border border-border p-4"
                data-repeater-row=(row)
            >
                (controls)
                <div class="flex flex-wrap justify-end gap-2">
                    (action("Move up", attributes! { cx => data-repeater-up="" }))
                    (action("Move down", attributes! { cx => data-repeater-down="" }))
                    (action("Remove", attributes! { cx => data-repeater-remove="" }))
                </div>
            </li>
        }
        .boxed())
    }
}

/// The key the row `row` of the repeater `key` posts its item's `leaf` under.
pub(crate) fn row_key(key: &str, row: &str, leaf: &str) -> String {
    format!("{key}.{row}.{leaf}")
}

/// The rows' keys as the repeater's one value.
fn encode(rows: &[HashMap<String, String>]) -> String {
    serde_json::to_string(rows).expect("a list of string maps serializes")
}

/// The rows' keys a repeater's value holds: none for a blank one, `None` for one that is not a
/// list of rows.
pub(crate) fn decode(value: &str) -> Option<Vec<HashMap<String, String>>> {
    if value.trim().is_empty() {
        return Some(Vec::new());
    }
    serde_json::from_str(value).ok()
}

/// The repeater's value holding `items`: what a record form hydrates the repeater with.
#[doc(hidden)]
pub fn write_items<T: RepeaterItem>(items: &[T]) -> String {
    let rows: Vec<HashMap<String, String>> = items
        .iter()
        .map(|item| {
            let mut out = HashMap::new();
            item.write(&mut out);
            out
        })
        .collect();
    encode(&rows)
}

/// The items the repeater `key` holds in a completed submission, none when it posts nothing.
///
/// # Errors
///
/// Each row's refusals, under the key the row's control posts, or the repeater's own when its
/// value holds no rows.
#[doc(hidden)]
pub fn parse_items<T: RepeaterItem>(
    cx: &Cx,
    key: &str,
    values: &HashMap<String, String>,
) -> std::result::Result<Vec<T>, Vec<FieldError>> {
    let Some(rows) = decode(values.get(key).map_or("", String::as_str)) else {
        return Err(vec![FieldError::invalid(key, "The rows could not be read")]);
    };
    let mut items = Vec::with_capacity(rows.len());
    let mut errors = Vec::new();
    for (row, values) in rows.iter().enumerate() {
        match T::parse(cx, values) {
            Ok(item) => items.push(item),
            Err(failures) => errors.extend(failures.into_iter().map(|failure| FieldError {
                key: row_key(key, &row.to_string(), &failure.key),
                kind: failure.kind,
            })),
        }
    }
    if errors.is_empty() {
        Ok(items)
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests;
