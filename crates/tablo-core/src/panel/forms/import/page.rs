//! The import page: the file input, the columns a file may name, and what refused the last file.

use topcoat::{
    context::Cx,
    icon::icon,
    view::{BoxView, ViewExt, attributes, view},
};

use super::{Column, FILE_PART, Refusal, RowError, columns};
use crate::{
    panel::url::import_template_url,
    resource::{Mounted, Resource},
};

/// What the import page shows of the resource and the request.
pub(super) struct ImportPage {
    label: String,
    plural_label: String,
    list_url: String,
    template_url: String,
    columns: Vec<Column>,
}

pub(super) fn page_of<R: Resource>(cx: &Cx, resource: &Mounted<R>) -> ImportPage {
    ImportPage {
        label: resource.label.clone(),
        plural_label: resource.plural_label.clone(),
        list_url: resource.url.clone(),
        template_url: import_template_url(&resource.url),
        columns: columns(&resource.form, &resource.form_mask(cx)),
    }
}

/// Renders the import page: the file input, the columns the file may name, and what refused the
/// last file, as a whole or row by row.
pub(super) fn render_import_page<'a>(
    cx: &'a Cx,
    page: ImportPage,
    refusal: Option<Refusal>,
    failures: Vec<RowError>,
) -> BoxView<'a> {
    let ImportPage {
        label,
        plural_label,
        list_url,
        template_url,
        columns,
    } = page;
    let csrf = crate::csrf::current_token(cx);
    // Posts to the current path, preserving a validated `?return=` target, as a form page does.
    let path = topcoat::router::request::uri(cx).path();
    let return_to = crate::panel::gate::return_target(cx);
    let action = match &return_to {
        Some(target) => crate::panel::url::with_return(path, target),
        None => path.to_string(),
    };
    let cancel = return_to.unwrap_or(list_url);
    let title = format!("Import {plural_label}");
    let description = format!(
        "Each row of the file creates one {}. The first row names the columns; a refused row \
         imports nothing, so correct the file and import it again.",
        label.to_lowercase()
    );
    let outline =
        tablo_ui::button_variants(tablo_ui::ButtonVariant::Outline, tablo_ui::ButtonSize::Md);
    let template_class = outline.clone();
    let refusal = refusal.map(|refusal| (refusal.kind(), refusal.message()));
    let failed = (!failures.is_empty()).then(|| {
        format!(
            "{} refused: nothing was imported",
            if failures.len() == 1 {
                "1 value was".to_string()
            } else {
                format!("{} values were", failures.len())
            }
        )
    });
    view! {
        cx =>
        tablo_ui::page(
            tablo_ui::page_header(
                tablo_ui::page_title((title))
                tablo_ui::page_description((description))
                tablo_ui::page_actions(
                    <a href=(template_url) class=(template_class)>
                        icon(data: tablo_ui::icons::DOWNLOAD)
                        "Download template"
                    </a>
                )
            )
            tablo_ui::page_content(
                <form
                    method="post"
                    action=(action)
                    enctype="multipart/form-data"
                    class="flex flex-col gap-4"
                >
                    (crate::csrf::field(cx, &csrf))
                    if let Some((kind, message)) = refusal {
                        tablo_ui::alert(
                            variant: tablo_ui::AlertVariant::Destructive,
                            attrs: attributes! { role="alert" data-import-refusal=(kind) },
                            tablo_ui::alert_title((message))
                        )
                    }
                    if let Some(failed) = failed {
                        tablo_ui::alert(
                            variant: tablo_ui::AlertVariant::Destructive,
                            attrs: attributes! { role="alert" },
                            tablo_ui::alert_title((failed))
                        )
                        tablo_ui::table(
                            tablo_ui::table_header(
                                tablo_ui::table_row(
                                    tablo_ui::table_head("Row")
                                    tablo_ui::table_head("Column")
                                    tablo_ui::table_head("Problem")
                                )
                            )
                            tablo_ui::table_body(
                                for failure in failures {
                                    tablo_ui::table_row(
                                        attrs: attributes! {
                                            data-import-error=(failure.row.to_string())
                                            data-import-key=(failure.key.unwrap_or_default())
                                        },
                                        tablo_ui::table_cell((failure.row.to_string()))
                                        tablo_ui::table_cell((failure.column))
                                        tablo_ui::table_cell((failure.message))
                                    )
                                }
                            )
                        )
                    }
                    tablo_ui::field(
                        tablo_ui::field_label(
                            attrs: attributes! { for="import-file" },
                            "CSV file"
                        )
                        tablo_ui::input(
                            attrs: attributes! {
                                id="import-file"
                                name=(FILE_PART)
                                type="file"
                                accept=".csv,text/csv"
                                required=""
                            }
                        )
                    )
                    <div class="flex items-center gap-2">
                        tablo_ui::button(
                            variant: tablo_ui::ButtonVariant::Primary,
                            attrs: attributes! { type="submit" },
                            icon(data: tablo_ui::icons::UPLOAD)
                            "Import"
                        )
                        <a
                            (crate::navigation::runtime_link(cx, &cancel))
                            class=(outline)
                        >
                            "Cancel"
                        </a>
                    </div>
                </form>
                tablo_ui::table(
                    tablo_ui::table_header(
                        tablo_ui::table_row(
                            tablo_ui::table_head("Column")
                            tablo_ui::table_head("Field")
                            tablo_ui::table_head("")
                        )
                    )
                    tablo_ui::table_body(
                        for column in columns {
                            tablo_ui::table_row(
                                attrs: attributes! { data-import-column=(column.key.clone()) },
                                tablo_ui::table_cell(<code>(column.key)</code>)
                                tablo_ui::table_cell((column.label))
                                tablo_ui::table_cell(
                                    if column.required {
                                        "Required"
                                    } else if column.multiple {
                                        "Repeat the column for each value"
                                    }
                                )
                            )
                        }
                    )
                )
            )
        )
    }
    .boxed()
}
