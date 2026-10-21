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
    decode::parse_file_body,
    submit::{Submission, Unposted, parse_form, prepare_submission, recheck_relationships},
    unique::{check_unique, taken_since},
};
use crate::{
    db::db,
    form::{FieldErrors, FormValues},
    resource::{Change, Resource},
    schema::{FormMask, Schema},
    topcoat_compat::async_page,
};

/// The most rows one CSV import creates: every row writes in the request's one transaction, so
/// a longer file is refused whole.
pub const MAX_IMPORT_ROWS: usize = 1_000;

/// The multipart part the file is posted under.
const FILE_PART: &str = "file";

const WRITE_IMPORT: &str = "import the file";

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
fn columns(schema: &Schema, mask: &FormMask) -> Vec<Column> {
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
            label: field.label_str().to_string(),
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
    UnknownColumns(Vec<String>),
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
            Self::UnknownColumns(_) => import::UNKNOWN_COLUMN,
            Self::RepeatedColumns(_) => import::REPEATED_COLUMN,
        }
    }

    fn message(&self) -> String {
        match self {
            Self::NoFile => "Choose a CSV file to import".to_string(),
            Self::NotUtf8 => "The file is not UTF-8 text: save it as CSV UTF-8".to_string(),
            Self::Unterminated { line } => {
                format!("A quoted cell opening on line {line} is never closed")
            }
            Self::NoHeader => "The file is empty: its first row names the columns".to_string(),
            Self::NoRows => "The file holds a header but no row".to_string(),
            Self::TooManyRows => format!("A file holds at most {MAX_IMPORT_ROWS} rows"),
            Self::UnknownColumns(names) => format!(
                "No field takes the column{} {}",
                if names.len() == 1 { "" } else { "s" },
                quoted(names)
            ),
            Self::RepeatedColumns(names) => {
                format!("The header names {} more than once", quoted(names))
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
/// its label ignoring case. Only a multiple choice's column may repeat, as its control posts its
/// key once per value.
fn resolve_header(header: &[String], columns: &[Column]) -> Result<Vec<Option<usize>>, Refusal> {
    let mut unknown = Vec::new();
    let mut repeated = Vec::new();
    let mut seen = HashSet::new();
    let mut slots = Vec::with_capacity(header.len());
    for name in header {
        let name = name.trim();
        if name.is_empty() {
            slots.push(None);
            continue;
        }
        let found = columns
            .iter()
            .position(|column| column.key == name)
            .or_else(|| {
                columns
                    .iter()
                    .position(|column| column.label.eq_ignore_ascii_case(name))
            });
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
    if !repeated.is_empty() {
        return Err(Refusal::RepeatedColumns(repeated));
    }
    Ok(slots)
}

/// A row of the file: its number as a spreadsheet shows it, and its cells.
type Row = (usize, Vec<String>);

/// The file's header and its rows, without the rows whose every cell is blank, which a
/// spreadsheet writes after the last one it fills.
fn read_file(parts: &FormParts) -> Result<(Vec<String>, Vec<Row>), Refusal> {
    let file = parts.files.get(FILE_PART).ok_or(Refusal::NoFile)?;
    let text = std::str::from_utf8(&file.bytes).map_err(|_| Refusal::NotUtf8)?;
    let mut records = csv::records(text)
        .map_err(|csv::Unterminated { line }| Refusal::Unterminated { line })?
        .into_iter();
    let header = records.next().ok_or(Refusal::NoHeader)?;
    // A spreadsheet numbers the header row 1.
    let rows: Vec<Row> = records
        .enumerate()
        .map(|(index, cells)| (index + 2, cells))
        .filter(|(_, cells)| cells.iter().any(|cell| !cell.trim().is_empty()))
        .collect();
    if rows.is_empty() {
        return Err(Refusal::NoRows);
    }
    if rows.len() > MAX_IMPORT_ROWS {
        return Err(Refusal::TooManyRows);
    }
    Ok((header, rows))
}

/// One refusal of one row, under the column it names.
struct RowError {
    /// The row's number as a spreadsheet shows it, the header being row 1.
    row: usize,
    /// The form key the error is under, `None` for the row as a whole or a key the request may
    /// not see.
    key: Option<String>,
    /// The header's spelling of the column, or the field's label when the header leaves it out.
    column: String,
    message: String,
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
            message: "The row holds a value under no column of the header".to_string(),
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

/// `errors` as `row`'s refusals, each under the header's spelling of its column. An error under a
/// key `mask` hides says only that a hidden field refuses the row, as the create form does.
fn row_errors(
    row: usize,
    errors: &FieldErrors,
    schema: &Schema,
    mask: &FormMask,
    header: &[(String, String)],
) -> Vec<RowError> {
    errors
        .iter()
        .map(|error| {
            if mask.hides(&error.key) {
                return RowError {
                    row,
                    key: None,
                    column: String::new(),
                    message: "A field you cannot see refuses this row".to_string(),
                };
            }
            let label = schema
                .fields()
                .find(|field| field.name() == error.key)
                .map_or(error.key.as_str(), |field| field.label_str());
            let column = header
                .iter()
                .find(|(key, _)| *key == error.key)
                .map_or(label, |(_, spelled)| spelled.as_str());
            RowError {
                row,
                key: Some(error.key.clone()),
                column: column.to_string(),
                message: error.message(label),
            }
        })
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
        let columns = columns(&resource.form, &resource.form_mask(cx));
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
            .header("x-content-type-options", "nosniff")
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
        let parts = parse_file_body(cx, body).await?;
        crate::csrf::verify(cx, parts.last(crate::csrf::FIELD_NAME))?;
        let mask = resource.form_mask(cx);
        let columns = columns(&resource.form, &mask);
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
        // Once a row is refused nothing is written, but every later row is still checked, so
        // the page lists what the whole file needs.
        let mut refused = !failures.is_empty()
            || staged
                .iter()
                .any(|(_, submission)| !submission.errors.is_empty());
        let mut db = db(cx);
        let mut tx = db.transaction().await.map_err(crate::error::unavailable)?;
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
                check_unique(cx, &resource, &schema, &values, &FormValues::new(), &mut tx).await?,
            );
            let form = parse_form(cx, &resource, &values, &mut errors)?;
            recheck_relationships(
                cx,
                &schema,
                &values,
                &FormValues::new(),
                &mut errors,
                &mut tx,
            )
            .await;
            if !errors.is_empty() {
                refused = true;
                failures.extend(row_errors(row, &errors, &resource.form, &mask, &spelled));
                continue;
            }
            let Some(form) = form.filter(|_| !refused) else {
                continue;
            };
            match R::create_record(cx, form, &mut tx).await {
                Ok(record) => {
                    // Each row is a create, so the hook a create runs inside its transaction
                    // runs once per row; an `Err` rolls the whole file back.
                    let change = Change::Create(record);
                    if let Err(error) = R::before_commit(cx, &change, &mut tx).await {
                        return Err(super::super::write::failed_write(cx, WRITE_IMPORT, error));
                    }
                    created.push(change);
                }
                Err(error) => {
                    drop(tx);
                    let taken =
                        taken_since(cx, &resource, &schema, &values, &FormValues::new(), &error)
                            .await;
                    if taken.is_empty() {
                        return Err(super::super::write::failed_write(cx, WRITE_IMPORT, error));
                    }
                    failures.extend(row_errors(row, &taken, &resource.form, &mask, &spelled));
                    failures.sort_by_key(|failure| failure.row);
                    return Ok(render_import_page(
                        cx,
                        page_of(cx, &resource),
                        None,
                        failures,
                    ));
                }
            }
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
        let note = format!(
            "Imported {count} {}",
            if count == 1 { "row" } else { "rows" }
        );
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
