//! The CSV import of an [`importable`](crate::ResourceDef::importable) resource: a page taking one
//! file, each row of which is a create submission, written in one transaction or not at all
//! (ADR 0031).

mod csv;
mod page;

use std::collections::HashSet;

use page::{page_of, render_import_page};
use topcoat::{
    Result,
    context::Cx,
    router::{Body, RouteFuture, error::forbidden},
    view::BoxView,
};

use super::{
    super::{gate::gate, write::commit_then},
    common::FormParts,
    submit::{
        Submission, Unposted, parse_form, prepare_submission, recheck_relationships, refusal,
    },
    unique::{check_unique, taken_since},
};
use crate::{
    Text,
    db::{WriteTx, db},
    form::{FieldErrors, FormValues},
    resource::{Change, Resource, WriteError},
    schema::{FormMask, Schema},
    text,
    topcoat_compat::async_page,
};

/// The most rows one CSV import creates: every row writes in the request's one transaction, so
/// a longer file is refused whole.
pub const MAX_IMPORT_ROWS: usize = 1_000;

/// The multipart part the file is posted under.
const FILE_PART: &str = "file";

/// The most cells one row of a file holds, so that a file of delimiters holds memory to a
/// bound.
const MAX_CELLS: usize = 500;

const WRITE_IMPORT: &str = "tablo-failed-import";

/// One column a request's import takes: a form key the create form posts as text.
struct Column {
    key: String,
    label: String,
    /// Whether the key's control posts a list, one value per column naming it.
    multiple: bool,
    /// Whether a row must fill it: the field has no blank answer and no default.
    required: bool,
}

/// The columns `mask` leaves the create form: every control but a file's, a repeater's and a
/// disabled one, none of which a cell can post.
fn columns(cx: &Cx, schema: &Schema, mask: &FormMask) -> Vec<Column> {
    schema
        .fields()
        .filter(|field| {
            !field.is_file()
                && field.as_repeater().is_none()
                && !field.is_disabled(false)
                && mask.writes(field.name()).is_none()
        })
        .map(|field| Column {
            key: field.name().to_string(),
            label: field.label().localize(cx),
            multiple: field.as_choice().is_some_and(|choice| choice.is_multiple()),
            required: field.is_required() && field.default_value().is_none(),
        })
        .collect()
}

/// Why a whole file is refused before any of its rows is read.
#[derive(Debug, PartialEq, Eq)]
enum Refusal {
    NoFile,
    NotUtf8,
    Unterminated { line: usize },
    NoHeader,
    NoRows,
    TooManyRows,
    TooManyCells { row: usize },
    UnknownColumns(Vec<String>),
    AmbiguousColumns(Vec<String>),
    RepeatedColumns(Vec<String>),
}

impl Refusal {
    /// The `data-import-refusal` hook naming the refusal.
    fn kind(&self) -> &'static str {
        use crate::dom::import;
        match self {
            Self::NoFile => import::NO_FILE,
            Self::NotUtf8 => import::ENCODING,
            Self::Unterminated { .. } => import::UNTERMINATED,
            Self::NoHeader => import::NO_HEADER,
            Self::NoRows => import::NO_ROWS,
            Self::TooManyRows => import::TOO_MANY_ROWS,
            Self::TooManyCells { .. } => import::TOO_MANY_CELLS,
            Self::UnknownColumns(_) => import::UNKNOWN_COLUMN,
            Self::AmbiguousColumns(_) => import::AMBIGUOUS_COLUMN,
            Self::RepeatedColumns(_) => import::REPEATED_COLUMN,
        }
    }

    fn message(&self) -> Text {
        match self {
            Self::NoFile => text!("tablo-import-no-file"),
            Self::NotUtf8 => text!("tablo-import-not-utf8"),
            Self::Unterminated { line } => text!("tablo-import-unterminated", line = *line),
            Self::NoHeader => text!("tablo-import-no-header"),
            Self::NoRows => text!("tablo-import-no-rows"),
            Self::TooManyRows => text!("tablo-import-too-many-rows", max = MAX_IMPORT_ROWS),
            Self::TooManyCells { row } => {
                text!("tablo-import-too-many-cells", row = *row, max = MAX_CELLS)
            }
            Self::UnknownColumns(names) => text!(
                "tablo-import-unknown-columns",
                count = names.len(),
                columns = quoted(names)
            ),
            Self::AmbiguousColumns(names) => {
                text!("tablo-import-ambiguous-columns", columns = quoted(names))
            }
            Self::RepeatedColumns(names) => {
                text!("tablo-import-repeated-columns", columns = quoted(names))
            }
        }
    }
}

fn quoted(names: &[String]) -> String {
    names
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The column each header cell names, `None` for a blank one: a cell matches a column's key, or
/// the one column whose label it spells ignoring case. Only a multiple choice's column may
/// repeat, as its control posts its key once per value.
fn resolve_header(header: &[String], columns: &[Column]) -> Result<Vec<Option<usize>>, Refusal> {
    let mut unknown = Vec::new();
    let mut ambiguous = Vec::new();
    let mut repeated = Vec::new();
    let mut seen = HashSet::new();
    let mut slots = Vec::with_capacity(header.len());
    for name in header {
        let name = name.trim();
        if name.is_empty() {
            slots.push(None);
            continue;
        }
        let found = match columns.iter().position(|column| column.key == name) {
            Some(index) => Some(index),
            None => {
                let lowered = name.to_lowercase();
                let mut labelled = columns
                    .iter()
                    .enumerate()
                    .filter(|(_, column)| column.label.to_lowercase() == lowered)
                    .map(|(index, _)| index);
                let first = labelled.next();
                if first.is_some() && labelled.next().is_some() {
                    ambiguous.push(name.to_string());
                    slots.push(None);
                    continue;
                }
                first
            }
        };
        match found {
            Some(index) => {
                if !seen.insert(index) && !columns[index].multiple {
                    repeated.push(name.to_string());
                }
                slots.push(Some(index));
            }
            None => {
                unknown.push(name.to_string());
                slots.push(None);
            }
        }
    }
    if !unknown.is_empty() {
        return Err(Refusal::UnknownColumns(unknown));
    }
    if !ambiguous.is_empty() {
        return Err(Refusal::AmbiguousColumns(ambiguous));
    }
    if !repeated.is_empty() {
        return Err(Refusal::RepeatedColumns(repeated));
    }
    Ok(slots)
}

/// The file's header and its rows, each numbered as a spreadsheet shows it, without the rows
/// whose every cell is blank, which a spreadsheet writes after the last one it fills.
fn read_file(parts: &FormParts) -> Result<(Vec<String>, Vec<csv::Record>), Refusal> {
    let file = parts.files.get(FILE_PART).ok_or(Refusal::NoFile)?;
    let text = std::str::from_utf8(&file.bytes).map_err(|_| Refusal::NotUtf8)?;
    let mut records = csv::records(text, MAX_IMPORT_ROWS + 1, MAX_CELLS)
        .map_err(|unread| match unread {
            csv::Unread::Unterminated { line } => Refusal::Unterminated { line },
            csv::Unread::TooManyRecords => Refusal::TooManyRows,
            csv::Unread::TooManyCells { record } => Refusal::TooManyCells { row: record },
        })?
        .into_iter();
    let (_, header) = records.next().ok_or(Refusal::NoHeader)?;
    let rows: Vec<csv::Record> = records.collect();
    if rows.is_empty() {
        return Err(Refusal::NoRows);
    }
    Ok((header, rows))
}

/// One refusal of one row, under the column it names.
struct RowError {
    /// The row's number as a spreadsheet shows it, counting the header and blank rows.
    row: usize,
    /// The form key the error is under, `None` for the row as a whole or a key the request may
    /// not see.
    key: Option<String>,
    /// The header's spelling of the column, or the field's label when the header leaves it out.
    column: String,
    message: Text,
}

/// What a row posts: each cell under its column's key, a missing trailing cell as a blank one.
/// A cell holding text under no column refuses the row.
fn row_pairs(
    row: usize,
    cells: &[String],
    slots: &[Option<usize>],
    columns: &[Column],
) -> Result<Vec<(String, String)>, RowError> {
    let stray = cells.iter().enumerate().any(|(index, cell)| {
        slots.get(index).copied().flatten().is_none() && !cell.trim().is_empty()
    });
    if stray {
        return Err(RowError {
            row,
            key: None,
            column: String::new(),
            message: text!("tablo-import-stray-value"),
        });
    }
    Ok(slots
        .iter()
        .enumerate()
        .filter_map(|(index, slot)| {
            let column = &columns[(*slot)?];
            let cell = cells.get(index).cloned().unwrap_or_default();
            Some((column.key.clone(), cell))
        })
        .collect())
}

/// `errors` as `row`'s refusals, each under the header's spelling of its column. The errors under
/// keys `mask` hides say once only that a hidden field refuses the row, as the create form does.
fn row_errors(
    cx: &Cx,
    row: usize,
    errors: &FieldErrors,
    schema: &Schema,
    mask: &FormMask,
    header: &[(String, String)],
) -> Vec<RowError> {
    let hidden = errors.iter().any(|error| mask.hides(&error.key));
    errors
        .iter()
        .filter(|error| !mask.hides(&error.key))
        .map(|error| {
            let label = schema
                .fields()
                .find(|field| field.name() == error.key)
                .map_or_else(|| Text::from(error.key.as_str()), crate::Field::label);
            let column = header
                .iter()
                .find(|(key, _)| *key == error.key)
                .map_or_else(|| label.localize(cx), |(_, spelled)| spelled.clone());
            RowError {
                row,
                key: Some(error.key.clone()),
                column,
                message: error.message(&label),
            }
        })
        .chain(hidden.then(|| RowError {
            row,
            key: None,
            column: String::new(),
            message: text!("tablo-import-unseen"),
        }))
        .collect()
}

/// Serves the import page of an importable resource, to a request that may create.
pub(crate) fn resource_import<R: Resource>(cx: &Cx, _body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can_create(cx) {
            return Err(forbidden().into());
        }
        crate::csrf::ensure_token(cx);
        Ok(render_import_page(
            cx,
            page_of(cx, &resource),
            None,
            Vec::new(),
        ))
    })
}

/// Serves a CSV file holding only the header the request's import takes.
pub(crate) fn resource_import_template<R: Resource>(cx: &Cx, _body: Body) -> RouteFuture<'_> {
    Box::pin(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can_create(cx) {
            return Err(forbidden().into());
        }
        let columns = columns(cx, &resource.form, &resource.form_mask(cx));
        let header: Vec<String> = columns
            .iter()
            .map(|column| {
                if column.key.contains([',', ';', '"', '\n', '\r']) {
                    format!("\"{}\"", column.key.replace('"', "\"\""))
                } else {
                    column.key.clone()
                }
            })
            .collect();
        let response = http::Response::builder()
            .status(200)
            .header(http::header::CONTENT_TYPE, "text/csv; charset=utf-8")
            .header(
                http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}-template.csv\"", resource.slug),
            )
            .body(Body::from(format!("{}\n", header.join(","))))?;
        Ok(response)
    })
}

/// Creates one record per row of the posted file through the create form's pipeline, in one
/// transaction: every row is written, or none is and the page lists each refusal.
pub(crate) fn resource_import_post<R: Resource>(cx: &Cx, body: Body) -> BoxView<'_> {
    async_page(async move {
        let resource = gate::<R>(cx)?;
        if !resource.can_create(cx) {
            return Err(forbidden().into());
        }
        let parts = crate::upload::parse_file_body(cx, body).await?;
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        let mask = resource.form_mask(cx);
        let columns = columns(cx, &resource.form, &mask);
        let refuse_file = |refusal: Refusal| {
            Ok(render_import_page(
                cx,
                page_of(cx, &resource),
                Some(refusal),
                Vec::new(),
            ))
        };
        let (header, rows) = match read_file(&parts) {
            Ok(file) => file,
            Err(refusal) => return refuse_file(refusal),
        };
        let slots = match resolve_header(&header, &columns) {
            Ok(slots) => slots,
            Err(refusal) => return refuse_file(refusal),
        };
        let spelled: Vec<(String, String)> = slots
            .iter()
            .zip(&header)
            .filter_map(|(slot, name)| {
                Some((columns[(*slot)?].key.clone(), name.trim().to_string()))
            })
            .collect();
        // Each row runs the steps a create runs before its transaction opens, so that no option
        // loader waits on the connection the transaction holds.
        let mut failures = Vec::new();
        let mut staged: Vec<(usize, Submission)> = Vec::with_capacity(rows.len());
        for (row, cells) in &rows {
            let pairs = match row_pairs(*row, cells, &slots, &columns) {
                Ok(pairs) => pairs,
                Err(error) => {
                    failures.push(error);
                    continue;
                }
            };
            let parts = FormParts {
                pairs,
                files: Default::default(),
                file_part_names: HashSet::new(),
            };
            let submission =
                prepare_submission(cx, &resource, parts, None, Unposted::Defaulted).await?;
            staged.push((*row, submission));
        }
        // Once a row is refused the transaction rolls back, but every later row is still
        // checked, and written when it passes, so that a later row's unique value is checked
        // against it: the page lists what the whole file needs.
        let mut refused = !failures.is_empty()
            || staged
                .iter()
                .any(|(_, submission)| !submission.errors.is_empty());
        let mut db = db(cx)?;
        let mut tx = WriteTx::begin(cx, &mut db)
            .await
            .map_err(crate::error::unavailable)?;
        let mut created = Vec::with_capacity(staged.len());
        for (row, submission) in staged {
            let Submission {
                schema,
                values,
                mut errors,
                ..
            } = submission;
            // Inside the transaction, so a row's unique value is checked against the rows
            // before it.
            errors.extend(
                check_unique(
                    cx,
                    &resource,
                    &schema,
                    &values,
                    &FormValues::new(),
                    &mut *tx,
                )
                .await?,
            );
            let form = parse_form(cx, &resource, &values, &mut errors)?;
            recheck_relationships(
                cx,
                &schema,
                &values,
                &FormValues::new(),
                &mut errors,
                &mut *tx,
            )
            .await;
            if !errors.is_empty() {
                refused = true;
                failures.extend(row_errors(
                    cx,
                    row,
                    &errors,
                    &resource.form,
                    &mask,
                    &spelled,
                ));
                continue;
            }
            let Some(form) = form else {
                continue;
            };
            // A savepoint per row, so a row its record fn refuses leaves nothing for the later
            // rows' checks to read.
            let mut savepoint = tx.transaction().await.map_err(crate::error::unavailable)?;
            let record = match R::create_record(cx, form, &mut savepoint).await {
                Ok(record) => {
                    savepoint
                        .commit()
                        .await
                        .map_err(crate::error::unavailable)?;
                    record
                }
                Err(WriteError::Refused(errors)) => {
                    savepoint
                        .rollback()
                        .await
                        .map_err(crate::error::unavailable)?;
                    refused = true;
                    let errors = refusal(&resource, "create_record", errors)?;
                    failures.extend(row_errors(
                        cx,
                        row,
                        &errors,
                        &resource.form,
                        &mask,
                        &spelled,
                    ));
                    continue;
                }
                Err(WriteError::Failed(error)) => {
                    drop(savepoint);
                    drop(tx);
                    let taken =
                        taken_since(cx, &resource, &schema, &values, &FormValues::new(), &error)
                            .await;
                    if taken.is_empty() {
                        return Err(super::super::write::failed_write(cx, WRITE_IMPORT, error));
                    }
                    failures.extend(row_errors(cx, row, &taken, &resource.form, &mask, &spelled));
                    failures.sort_by_key(|failure| failure.row);
                    return Ok(render_import_page(
                        cx,
                        page_of(cx, &resource),
                        None,
                        failures,
                    ));
                }
            };
            if refused {
                continue;
            }
            // Each row is a create, so the hook a create runs inside its transaction runs once
            // per row; an `Err` rolls the whole file back.
            let change = Change::Create(record);
            if let Err(error) = R::before_commit(cx, &change, &mut *tx).await {
                return Err(super::super::write::failed_write(cx, WRITE_IMPORT, error));
            }
            created.push(change);
        }
        if refused {
            drop(tx);
            failures.sort_by_key(|failure| failure.row);
            return Ok(render_import_page(
                cx,
                page_of(cx, &resource),
                None,
                failures,
            ));
        }
        let count = created.len();
        let note = crate::t!(cx, "tablo-import-imported", count = count);
        commit_then(
            cx,
            tx,
            Ok((created, crate::Outcome::done())),
            |changes| async move {
                for change in changes {
                    crate::resource::run_after_commit::<R>(cx, change).await;
                }
            },
            note,
            WRITE_IMPORT,
            &resource.url,
        )
        .await
    })
}

#[cfg(test)]
mod tests;
