//! The [`AuditLog`](super::AuditLog)'s table and detail: the columns that spell an entry in the
//! reader's locale, and mask what the reader may not see.

use topcoat::{context::Cx, view::*};

use super::super::state::current;
use crate::{
    DateColumn, Detail, Table, Text, TextColumn,
    audit::{AuditChange, AuditEntry},
    extend::Column,
};

/// The log's list: newest first, searched by user, event and record.
pub(super) fn table() -> Table<AuditEntry> {
    Table::new((
        DateColumn::new(crate::lens!(AuditEntry.created_at))
            .label(Text::message("tablo-audit-when"))
            .sortable(),
        TextColumn::new(crate::lens!(AuditEntry.actor))
            .label(Text::message("tablo-audit-actor"))
            .searchable(),
        Event,
        ResourceName,
        record(),
        Changes,
    ))
    .default_sort_desc(AuditEntry::fields().created_at())
}

/// The entry's page: every column of the list.
pub(super) fn detail() -> Detail<AuditEntry> {
    Detail::empty()
        .column(
            DateColumn::new(crate::lens!(AuditEntry.created_at))
                .label(Text::message("tablo-audit-when")),
        )
        .column(
            TextColumn::new(crate::lens!(AuditEntry.actor))
                .label(Text::message("tablo-audit-actor")),
        )
        .column(Event)
        .column(ResourceName)
        .column(record())
        .column(Changes)
}

/// The record's key, linking its page while the entry kept one inside the app.
fn record() -> TextColumn<AuditEntry, Option<String>> {
    TextColumn::new(crate::lens!(AuditEntry.record))
        .label(Text::message("tablo-audit-record"))
        .searchable()
        .url(|entry| entry.url.clone().filter(|url| is_local(url)))
}

/// Whether `url` is a path on this origin: an app's own event may store any text.
fn is_local(url: &str) -> bool {
    url.starts_with('/') && !url.starts_with("//") && !url.starts_with("/\\")
}

/// What happened, in the reader's words: "Updated", or an app event's own name.
struct Event;

impl Column<AuditEntry> for Event {
    fn name(&self) -> &str {
        "event"
    }

    fn label(&self) -> Text {
        Text::message("tablo-audit-event")
    }

    fn text(&self, cx: &Cx, row: &AuditEntry) -> String {
        let built_in = |id| crate::t!(cx, id);
        match row.event.as_str() {
            "create" => built_in("tablo-audit-create"),
            "update" => built_in("tablo-audit-update"),
            "delete" => built_in("tablo-audit-delete"),
            "restore" => built_in("tablo-audit-restore"),
            "force_delete" => built_in("tablo-audit-force-delete"),
            "attach" => built_in("tablo-audit-attach"),
            "detach" => built_in("tablo-audit-detach"),
            event => match event
                .strip_prefix("action:")
                .or_else(|| event.strip_prefix("header_action:"))
            {
                Some(name) => crate::t!(cx, "tablo-audit-action", name = name.to_owned()),
                None => event.to_owned(),
            },
        }
    }

    fn search_expr(&self, term: &str) -> Option<toasty::stmt::Expr<bool>> {
        crate::column::contains_expr(&crate::lens!(AuditEntry.event), term)
    }
}

/// The resource the entry is about, by its plural label while the panel still mounts it.
struct ResourceName;

impl Column<AuditEntry> for ResourceName {
    fn name(&self) -> &str {
        "resource"
    }

    fn label(&self) -> Text {
        Text::message("tablo-audit-resource")
    }

    fn text(&self, cx: &Cx, row: &AuditEntry) -> String {
        let Some(slug) = row.resource.as_deref() else {
            return String::new();
        };
        current(cx)
            .and_then(|panel| panel.children.values().find(|child| child.slug == slug))
            .map_or_else(|| slug.to_owned(), |child| child.plural_label.localize(cx))
    }
}

/// The fields the entry changed, each from its value before to its value after; a field the
/// reader may not view on the entry's resource reads "Hidden".
struct Changes;

impl Changes {
    /// The entry's changes as `cx` may read them: a hidden field keeps its label and loses its
    /// values.
    fn visible(cx: &Cx, row: &AuditEntry) -> Vec<(String, Option<(String, String)>)> {
        let hidden = hidden_fields(cx, row.resource.as_deref());
        row.changes
            .0
            .iter()
            .map(|change| {
                let masked = hidden.as_ref().is_none_or(|hidden| {
                    hidden.iter().any(|field| {
                        change.field == *field || change.field.starts_with(&format!("{field}."))
                    })
                });
                let values = (!masked).then(|| spell(change));
                (change.label.clone(), values)
            })
            .collect()
    }
}

/// `change`'s values, an empty one spelled "—".
fn spell(change: &AuditChange) -> (String, String) {
    let or_dash = |value: &Option<String>| value.clone().unwrap_or_else(|| "—".to_owned());
    (or_dash(&change.before), or_dash(&change.after))
}

/// The fields `cx` may not view on the resource `slug` names, over every resource the panel
/// mounts at that slug; `None` when the panel mounts none, which hides every field.
fn hidden_fields(cx: &Cx, slug: Option<&str>) -> Option<Vec<String>> {
    let Some(slug) = slug else {
        // An app event on no resource names what the app chose to show.
        return Some(Vec::new());
    };
    let panel = current(cx)?;
    let mut mounted = panel
        .children
        .values()
        .filter(|child| child.slug == slug)
        .peekable();
    mounted.peek()?;
    let mut hidden = Vec::new();
    for child in mounted {
        hidden.extend((child.hidden)(cx)?);
    }
    Some(hidden)
}

impl Column<AuditEntry> for Changes {
    fn name(&self) -> &str {
        "changes"
    }

    fn label(&self) -> Text {
        Text::message("tablo-audit-changes")
    }

    fn text(&self, cx: &Cx, row: &AuditEntry) -> String {
        let hidden = crate::t!(cx, "tablo-audit-hidden");
        Self::visible(cx, row)
            .into_iter()
            .map(|(label, values)| match values {
                Some((before, after)) => format!("{label}: {before} → {after}"),
                None => format!("{label}: {hidden}"),
            })
            .collect::<Vec<_>>()
            .join("; ")
    }

    fn cell<'a>(&self, cx: &'a Cx, row: &AuditEntry) -> BoxView<'a> {
        let hidden = crate::t!(cx, "tablo-audit-hidden");
        let items: Vec<BoxView<'a>> = Self::visible(cx, row)
            .into_iter()
            .map(|(label, values)| {
                let values = match values {
                    Some((before, after)) => view! {
                        <span class="text-muted-foreground line-through">(before)</span>
                        " → "
                        <span>(after)</span>
                    }
                    .boxed(),
                    None => {
                        let hidden = hidden.clone();
                        view! {
                            <span class="text-muted-foreground italic">(hidden)</span>
                        }
                        .boxed()
                    }
                };
                view! {
                    <li data-audit-field=(label.clone())>
                        <span class="font-medium">
                            (label)
                            ": "
                        </span>
                        (values)
                    </li>
                }
                .boxed()
            })
            .collect();
        view! {
            <ul class="flex flex-col gap-0.5 text-sm">
                for item in items {
                    (item)
                }
            </ul>
        }
        .boxed()
    }
}
