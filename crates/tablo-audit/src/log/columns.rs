//! The [`AuditLog`](super::AuditLog)'s table and detail: the columns that spell an entry in the
//! reader's locale, and mask what the reader may not see.

use tablo_core::{
    DateColumn, Detail, Table, Text, TextColumn, contains_expr, extend::Column, lens,
    mounted_resources, t,
};
use topcoat::{context::Cx, view::*};

use crate::{AuditChange, AuditEntry};

/// The log's list: newest first, searched by user, event and record.
pub(super) fn table() -> Table<AuditEntry> {
    Table::new((
        DateColumn::new(lens!(AuditEntry.created_at))
            .label(Text::message("tablo-audit-when"))
            .sortable(),
        TextColumn::new(lens!(AuditEntry.actor))
            .label(Text::message("tablo-audit-actor"))
            .searchable(),
    ))
    .column(Event)
    .column(ResourceName)
    .column(record())
    .column(Changes)
    .default_sort_desc(AuditEntry::fields().created_at())
}

/// The entry's page: every column of the list.
pub(super) fn detail() -> Detail<AuditEntry> {
    Detail::empty()
        .column(
            DateColumn::new(lens!(AuditEntry.created_at)).label(Text::message("tablo-audit-when")),
        )
        .column(TextColumn::new(lens!(AuditEntry.actor)).label(Text::message("tablo-audit-actor")))
        .column(Event)
        .column(ResourceName)
        .column(record())
        .column(Changes)
}

/// The record's key, linking its page while the entry kept one inside the app.
fn record() -> TextColumn<AuditEntry, Option<String>> {
    TextColumn::new(lens!(AuditEntry.record))
        .label(Text::message("tablo-audit-record"))
        .searchable()
        .url(|_, entry| entry.url.clone().filter(|url| is_local(url)))
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
        let built_in = |id| t!(cx, id);
        match row.event.as_str() {
            "create" => built_in("tablo-audit-create"),
            "update" => built_in("tablo-audit-update"),
            "delete" => built_in("tablo-audit-delete"),
            "restore" => built_in("tablo-audit-restore"),
            "force_delete" => built_in("tablo-audit-force-delete"),
            "attach" => built_in("tablo-audit-attach"),
            "detach" => built_in("tablo-audit-detach"),
            "impersonate" => built_in("tablo-audit-impersonate"),
            "stop_impersonating" => built_in("tablo-audit-stop-impersonating"),
            "sign_in" => built_in("tablo-audit-sign-in"),
            "sign_in_failed" => built_in("tablo-audit-sign-in-failed"),
            "second_factor_failed" => built_in("tablo-audit-second-factor-failed"),
            "sign_out" => built_in("tablo-audit-sign-out"),
            "credentials_changed" => built_in("tablo-audit-credentials-changed"),
            "password_reset" => built_in("tablo-audit-password-reset"),
            "email_changed" => built_in("tablo-audit-email-changed"),
            "two_factor_enabled" => built_in("tablo-audit-two-factor-enabled"),
            "two_factor_disabled" => built_in("tablo-audit-two-factor-disabled"),
            "recovery_codes_replaced" => built_in("tablo-audit-recovery-codes-replaced"),
            event => match event
                .strip_prefix("action:")
                .or_else(|| event.strip_prefix("header_action:"))
            {
                Some(name) => t!(cx, "tablo-audit-action", name = name.to_owned()),
                None => event.to_owned(),
            },
        }
    }

    fn search_expr(&self, term: &str) -> Option<toasty::stmt::Expr<bool>> {
        contains_expr(&lens!(AuditEntry.event), term)
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
        mounted_resources(cx)
            .into_iter()
            .find(|resource| resource.slug() == slug)
            .map_or_else(
                || slug.to_owned(),
                |resource| resource.plural_label().localize(cx),
            )
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
                (
                    auth_label(cx, row, change).unwrap_or_else(|| change.label.clone()),
                    values,
                )
            })
            .collect()
    }
}

/// The label of a field an auth entry records, in the reader's words: the trail may write it
/// where the request does not read the panel's catalog yet, as a sign-in or a lapsed session.
fn auth_label(cx: &Cx, row: &AuditEntry, change: &AuditChange) -> Option<String> {
    // An app's own event may name a field alike: only the trail's entries are relabelled.
    let auth = row.event.contains("impersonat")
        || matches!(
            row.event.as_str(),
            "sign_in"
                | "sign_in_failed"
                | "second_factor_failed"
                | "sign_out"
                | "credentials_changed"
                | "password_reset"
                | "email_changed"
                | "two_factor_enabled"
                | "two_factor_disabled"
                | "recovery_codes_replaced"
        );
    if row.resource.is_some() || !auth {
        return None;
    }
    match change.field.as_str() {
        "user" => Some(t!(cx, "tablo-audit-target")),
        "until" => Some(t!(cx, "tablo-audit-until")),
        "login" => Some(t!(cx, "tablo-audit-login")),
        _ => None,
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
    let mut mounted = mounted_resources(cx)
        .into_iter()
        .filter(|resource| resource.slug() == slug)
        .peekable();
    mounted.peek()?;
    let mut hidden = Vec::new();
    for resource in mounted {
        hidden.extend(resource.hidden_fields(cx)?);
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
        let hidden = t!(cx, "tablo-audit-hidden");
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
        let hidden = t!(cx, "tablo-audit-hidden");
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
