//! Repeaters: a list of values stored in one `#[document]` column, edited one row per item.
//!
//! Each row posts its item's keys under `{key}.{row}.`, and the repeater's own key posts the rows
//! in the order they show, so a row the browser adds, removes or moves needs no other key.
//! [`Schema::read`] reads them into the repeater's key as [`FormValue::Rows`], each row holding its
//! item's values: the edit's stored value, a hidden repeater and the parse then see one key like
//! any other field's.
//!
//! The browser adds a row by copying the blank row the repeater renders in a `<template>`, its
//! keys carrying [`ROW`] for the row's number (`assets/repeaters.js`).

use std::collections::HashSet;

use tablo_ui::{
    ButtonSize, ButtonVariant, FieldLegendVariant, button, field_error as ui_field_error,
    field_legend, field_set,
};
use topcoat::{Result, context::Cx, view::*};

use super::{
    FormInput, Schema,
    fields::{Field, Items},
    read::Pairs,
    tree::Source,
};
use crate::values::{FieldError, FieldErrors, FormValue, FormValues};

/// What the blank row's keys carry for the number the browser gives a row it adds.
pub(crate) const ROW: &str = "__row__";

/// The most rows one repeater takes: each renders in full when the form comes back refused, so
/// an unbounded list would let a small submission render a page of any size.
pub const MAX_REPEATER_ROWS: usize = 100;

/// A repeater's control: its item's controls, one row each.
pub(crate) struct RepeaterControl {
    item: fn() -> Schema,
    /// The add button's label, over "Add item".
    pub(crate) add_label: Option<String>,
    /// How many rows it takes, set by `min_items` and `max_items`.
    pub(crate) bounds: Items,
}

impl RepeaterControl {
    /// One row's controls.
    pub(crate) fn item_schema(&self) -> Schema {
        (self.item)()
    }

    /// A repeater whose rows hold `item`'s controls.
    #[cfg(test)]
    pub(crate) fn of(item: fn() -> Schema) -> Self {
        Self {
            item,
            add_label: None,
            bounds: Items::default(),
        }
    }

    pub(crate) fn new<T: FormInput>() -> Self {
        Self {
            item: T::schema,
            add_label: None,
            bounds: Items::default(),
        }
    }

    /// Reads the rows posted under `key`, in the order `key` lists them, each through its item's
    /// schema. A row's key that its item does not declare, or that a row `key` does not list
    /// posts, stays unclaimed for the unknown-key check to refuse. `None` when the submission does
    /// not post `key`, so an edit keeps the stored rows.
    ///
    /// # Errors
    ///
    /// `key` lists something other than row numbers, a row twice, a row posting none of its
    /// item's keys, or more than [`MAX_REPEATER_ROWS`] rows. A browser posts every control of each
    /// row it shows, so none of these comes from the form.
    pub(super) fn read(
        &self,
        key: &str,
        pairs: &mut Pairs<'_>,
    ) -> std::result::Result<Option<FormValue>, String> {
        let Some(order) = pairs.claim(key).pop() else {
            return Ok(None);
        };
        let item = (self.item)();
        let mut seen = HashSet::new();
        let mut rows = Vec::new();
        for token in order.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            if rows.len() == MAX_REPEATER_ROWS {
                return Err(format!("`{key}` lists more than {MAX_REPEATER_ROWS} rows"));
            }
            let row: usize = token
                .parse()
                .map_err(|_| format!("`{key}` lists `{token}`, which is not a row"))?;
            if !seen.insert(row) {
                return Err(format!("`{key}` lists row {row} twice"));
            }
            let mut posted = Vec::new();
            for leaf in item.fields().map(Field::name) {
                for value in pairs.claim(&row_key(key, &row.to_string(), leaf)) {
                    posted.push((leaf.to_string(), value.to_string()));
                }
            }
            if posted.is_empty() {
                return Err(format!("`{key}` lists row {row}, which posts nothing"));
            }
            rows.push(item.read_in(&posted, pairs.zone.clone())?);
        }
        Ok(Some(FormValue::Rows(rows)))
    }

    /// Adds to `errors` what the item's controls' own rules refuse in each of `rows`, under the
    /// key the row's control posts: a choice that is not one of its options, an email field's
    /// address, a text field's declared length, bounds and pattern. Each row checks against the
    /// row `stored` holds at its position, so a value the record stores there is not checked
    /// again.
    pub(crate) async fn check(
        &self,
        cx: &Cx,
        key: &str,
        rows: &[FormValues],
        stored: &[FormValues],
        errors: &mut FieldErrors,
    ) {
        for (row, item) in rows.iter().enumerate() {
            let (schema, values) = self.row(key, &row.to_string(), item);
            let held = stored.get(row).map_or_else(FormValues::new, |held| {
                self.row(key, &row.to_string(), held).1
            });
            Box::pin(schema.check_controls(cx, &values, &held, errors)).await;
        }
    }

    /// The item's schema posting under the row `row` of the repeater `key`, and `item`'s keys as
    /// that row posts them.
    fn row(&self, key: &str, row: &str, item: &FormValues) -> (Schema, FormValues) {
        let prefix = row_key(key, row, "");
        let mut schema = (self.item)();
        for field in &mut schema.fields {
            field.prefix(&prefix);
        }
        let values = item
            .iter()
            .map(|(leaf, value)| (format!("{prefix}{leaf}"), value.clone()))
            .collect();
        (schema, values)
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
        let items = source.rows(&key);
        let mut rows = Vec::with_capacity(items.len());
        // A row's Remove button is disabled while the list holds no more rows than it needs;
        // `assets/repeaters.js` keeps it so as rows come and go.
        let removable = self.bounds.min.is_none_or(|min| items.len() > min);
        for (row, item) in items.iter().enumerate() {
            let errors = source.errors();
            rows.push(
                self.render_row(cx, &key, &row.to_string(), item, errors, removable, source)
                    .await?,
            );
        }
        let blank = self
            .render_row(
                cx,
                &key,
                ROW,
                &FormValues::new(),
                &FieldErrors::new(),
                true,
                source,
            )
            .await?;
        let order = (0..items.len())
            .map(|row| row.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let next = items.len().to_string();
        let most = self
            .bounds
            .max
            .map_or(MAX_REPEATER_ROWS, |max| max.min(MAX_REPEATER_ROWS));
        let full = items.len() >= most;
        let min = self.bounds.min;
        let label = field.label_str().to_string();
        let add = self
            .add_label
            .clone()
            .unwrap_or_else(|| "Add item".to_string());
        let error = source
            .error_for(field)
            .map(|error| (error.message(&label), super::fields::error_kind(error)));
        let legend_id = format!("{}-legend", source.id(&key));
        // Named as every control's error slot is, so the fieldset describes itself by it.
        let error_id = format!("{}-error", source.id(&key));
        let described_by = error.is_some().then(|| error_id.clone());
        Ok(view! {
            cx =>
            field_set(
                attrs: attributes! {
                    class="ac-field gap-3"
                    data-repeater=(key.clone())
                    data-repeater-next=(next)
                    data-repeater-max=(most)
                    data-repeater-min=(min)
                    aria-describedby=(described_by)
                },
                field_legend(
                    variant: FieldLegendVariant::Label,
                    attrs: attributes! { id=(legend_id.clone()) class="mb-0" },
                    (label)
                )
                <input type="hidden" name=(key) value=(order) data-repeater-order="">
                if let Some((message, kind)) = error {
                    ui_field_error(
                        attrs: attributes! { id=(error_id) class="ac-error" data-error=(kind) },
                        (message)
                    )
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
                        attrs: attributes! { type="button" data-repeater-add="" disabled=(full) },
                        (add)
                    )
                </div>
            )
        }
        .boxed())
    }

    /// Renders the row `row` of the repeater `key` holding `item`, its controls posting under the
    /// row's prefix, with the buttons that move and remove it, Remove disabled unless
    /// `removable`.
    #[expect(
        clippy::too_many_arguments,
        reason = "one call site renders a stored row, the other the blank one"
    )]
    async fn render_row<'a>(
        &self,
        cx: &'a Cx,
        key: &str,
        row: &str,
        item: &FormValues,
        errors: &FieldErrors,
        removable: bool,
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let (schema, values) = self.row(key, row, item);
        let mut rendered = Source::form(&values, errors).scoped(source.scope());
        if let Some(options) = source.options() {
            rendered = rendered.options_at(options);
        }
        let controls = schema.render(cx, rendered).await?;
        let row = row.to_string();
        let action = |label: &'static str, data: Attributes, disabled: bool| {
            view! {
                cx =>
                button(
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    attrs: attributes! { type="button" disabled=(disabled) (data) },
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
                    (action("Move up", attributes! { cx => data-repeater-up="" }, false))
                    (action(
                        "Move down",
                        attributes! { cx => data-repeater-down="" },
                        false,
                    ))
                    (action(
                        "Remove",
                        attributes! { cx => data-repeater-remove="" },
                        !removable,
                    ))
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

/// The repeater's value holding `items`: what a record form hydrates the repeater with, and what
/// a hand-written one writes.
pub fn write_items<T: FormInput>(items: &[T]) -> FormValue {
    FormValue::Rows(
        items
            .iter()
            .map(|item| {
                let mut out = FormValues::new();
                item.write(&mut out);
                out
            })
            .collect(),
    )
}

/// The items the repeater `key` holds in a submission [read](crate::Schema::read) through its
/// schema, none when it posts nothing: what a record form's parse reads the repeater with, and
/// what a hand-written one or an action input reads.
///
/// # Errors
///
/// Each row's refusals, under the key the row's control posts, or the repeater's own when it holds
/// text or a list.
pub fn parse_items<T: FormInput>(
    cx: &Cx,
    key: &str,
    values: &FormValues,
) -> std::result::Result<Vec<T>, Vec<FieldError>> {
    let rows: &[FormValues] = match values.get(key) {
        None => &[],
        Some(FormValue::Rows(rows)) => rows,
        Some(_) => return Err(vec![crate::values::unreadable(key)]),
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
