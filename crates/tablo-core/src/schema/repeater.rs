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
use topcoat::{Result, context::Cx, icon::icon, view::*};

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
    pub(crate) add_label: Option<crate::Text>,
    /// How many rows it takes, set by `min_items` and `max_items`.
    pub(crate) bounds: Items,
    /// Whether the rows render as a table, set by `table`.
    pub(crate) table: bool,
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
            table: false,
        }
    }

    pub(crate) fn new<T: FormInput>() -> Self {
        Self {
            item: T::schema,
            add_label: None,
            bounds: Items::default(),
            table: false,
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
            let row = row.to_string();
            rows.push(if self.table {
                self.render_table_row(cx, &key, &row, item, errors, removable, source)
                    .await?
            } else {
                self.render_row(cx, &key, &row, item, errors, removable, source)
                    .await?
            });
        }
        let blank_errors = FieldErrors::new();
        let blank = if self.table {
            self.render_table_row(
                cx,
                &key,
                ROW,
                &FormValues::new(),
                &blank_errors,
                true,
                source,
            )
            .await?
        } else {
            self.render_row(
                cx,
                &key,
                ROW,
                &FormValues::new(),
                &blank_errors,
                true,
                source,
            )
            .await?
        };
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
        let label = field.label();
        let add = self
            .add_label
            .clone()
            .unwrap_or_else(|| crate::Text::message("tablo-repeater-add"));
        let error = source.error_for(field).map(|error| {
            (
                error.message(&label).localize(cx),
                super::fields::error_kind(error),
            )
        });
        let head = self.table.then(|| self.table_head(cx));
        let legend_id = format!("{}-legend", source.id(&key));
        // Named as every control's error slot is, so the fieldset describes itself by it.
        let error_id = format!("{}-error", source.id(&key));
        let described_by = error.is_some().then(|| error_id.clone());
        Ok(view! {
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
                if let Some(head) = head {
                    <div class=(TABLE_BOX)>
                        <div class="overflow-x-auto">
                            <table class="w-full border-collapse text-sm">
                                (head)
                                <tbody
                                    class="divide-y divide-border"
                                    aria-labelledby=(legend_id)
                                    data-repeater-rows=""
                                >
                                    for row in rows {
                                        (row)
                                    }
                                </tbody>
                            </table>
                        </div>
                        <template data-repeater-blank="">(blank)</template>
                        <div class="flex justify-center border-t border-border p-1.5">
                            button(
                                variant: ButtonVariant::Ghost,
                                size: ButtonSize::Sm,
                                attrs: attributes! {
                                    type="button"
                                    class="text-primary hover:text-primary"
                                    data-repeater-add=""
                                    disabled=(full)
                                },
                                icon(
                                    data: tablo_ui::icons::PLUS,
                                    attrs: attributes! { aria-hidden="true" }
                                )
                                (add)
                            )
                        </div>
                    </div>
                } else {
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
                }
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
        let action = move |id: &'static str, data: Attributes, disabled: bool| {
            let label = crate::t!(cx, id);
            view! {
                button(
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    attrs: attributes! { type="button" disabled=(disabled) (data) },
                    (label)
                )
            }
        };
        Ok(view! {
            <li
                class="flex flex-col gap-4 rounded-lg border border-border p-4"
                data-repeater-row=(row)
            >
                (controls)
                <div class="flex flex-wrap justify-end gap-2">
                    (action(
                        "tablo-repeater-up",
                        attributes! { data-repeater-up="" },
                        false,
                    ))
                    (action(
                        "tablo-repeater-down",
                        attributes! { data-repeater-down="" },
                        false,
                    ))
                    (action(
                        "tablo-repeater-remove",
                        attributes! { data-repeater-remove="" },
                        !removable,
                    ))
                </div>
            </li>
        }
        .boxed())
    }
}

impl RepeaterControl {
    /// The table's header: a column per control of the item, labelled by it, and one for the
    /// row's buttons.
    fn table_head<'a>(&self, cx: &'a Cx) -> BoxView<'a> {
        let item = (self.item)();
        let labels: Vec<_> = item.cell_labels();
        let actions = crate::t!(cx, "tablo-repeater-actions");
        view! {
            <thead class="bg-muted/50">
                <tr class="border-b border-border">
                    for (label, required) in labels {
                        <th
                            scope="col"
                            class="px-3 py-2 text-start font-medium text-foreground"
                        >
                            (label)
                            if required {
                                <span class="ms-0.5 text-destructive" aria-hidden="true">
                                    "*"
                                </span>
                            }
                        </th>
                    }
                    <th scope="col" class="w-px p-0">
                        <span class="sr-only">(actions)</span>
                    </th>
                </tr>
            </thead>
        }
        .boxed()
    }

    /// Renders the row `row` of the repeater `key` holding `item` as a table row: a cell per
    /// control, its label read by a screen reader alone since the column's header shows it, then
    /// the buttons that move and remove it, Remove disabled unless `removable`.
    #[expect(
        clippy::too_many_arguments,
        reason = "one call site renders a stored row, the other the blank one"
    )]
    async fn render_table_row<'a>(
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
        let cells = schema.render_cells(cx, rendered).await?;
        let row = row.to_string();
        let action = move |id: &'static str, glyph, data: Attributes, disabled: bool| {
            let label = crate::t!(cx, id);
            view! {
                button(
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Icon,
                    attrs: attributes! {
                        type="button"
                        class="size-8 text-muted-foreground"
                        aria-label=(label.clone())
                        title=(label)
                        disabled=(disabled)
                        (data)
                    },
                    icon(data: glyph, attrs: attributes! { aria-hidden="true" })
                )
            }
        };
        Ok(view! {
            <tr class=(TABLE_ROW) data-repeater-row=(row)>
                for cell in cells {
                    <td class="border-e border-border p-0 align-top">(cell)</td>
                }
                <td class="w-px px-1 py-0.5 align-top whitespace-nowrap">
                    (action(
                        "tablo-repeater-up",
                        tablo_ui::icons::ARROW_UP,
                        attributes! { data-repeater-up="" },
                        false,
                    ))
                    (action(
                        "tablo-repeater-down",
                        tablo_ui::icons::ARROW_DOWN,
                        attributes! { data-repeater-down="" },
                        false,
                    ))
                    (action(
                        "tablo-repeater-remove",
                        tablo_ui::icons::TRASH,
                        attributes! { data-repeater-remove="" class="hover:text-destructive" },
                        !removable,
                    ))
                </td>
            </tr>
        }
        .boxed())
    }
}

/// Renders a table repeater's box: one border around the header, the rows and the add button.
const TABLE_BOX: &str = "overflow-hidden rounded-lg border border-border bg-background";

/// Renders a table repeater's row: each control fills its cell, borderless, its focus ring drawn
/// inside the cell, its label left to a screen reader and its error under it.
const TABLE_ROW: &str = "[&_[data-slot=field-label]]:sr-only [&_[data-slot=field]]:gap-0 \
     [&_:is(input,select,textarea)]:rounded-none [&_:is(input,select,textarea)]:border-0 \
     [&_:is(input,select,textarea)]:bg-transparent [&_:is(input,select,textarea)]:shadow-none \
     [&_:is(input,select,textarea)]:focus-visible:ring-inset \
     [&_:is(input,select,textarea)]:focus-visible:ring-offset-0 [&_.ac-error]:px-3 \
     [&_.ac-error]:pb-2";

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
