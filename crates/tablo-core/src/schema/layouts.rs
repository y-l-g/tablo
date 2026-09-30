//! Layout containers — `Section`, `Group`, `Grid`, `Repeater`.
//!
//! The compositional seams for form layout; each holds a child `Schema`
//! rendered through the tree walk.

use tablo_ui::{
    card_content, card_header, card_title, field_error as ui_field_error,
    field_group as ui_field_group,
};
use topcoat::{
    Result,
    context::Cx,
    view::{StaticClass, class, *},
};

use super::{
    Schema,
    fields::Field,
    tree::{IntoSchema, Mode, Source, render_nodes},
};

/// The one titled-group container: `Section` and `Repeater` render the same
/// border-only panel, so every titled group on a form looks alike.
///
/// The shape is the `card` primitive's — rounded panel with header/content
/// rhythm — without its opaque paint: no `bg-card`, no `shadow-sm`. Overlays
/// keep the primitive as-is (dialogs, sheets and popovers sit above the page
/// and need the fill and the shadow); a form panel sits on the page
/// background, so it draws only its border.
const PANEL: StaticClass =
    class!("flex flex-col gap-5 rounded-xl border border-border py-6 text-card-foreground");

/// Section — titled container with an optional child `Schema`.
///
/// The single customization seam for form layout in v1: additive `class` is
/// allowed on the panel container only (narrow seam, no per-field `attrs`).
/// This keeps Token editing in `styles.css` as the primary theming mechanism.
#[derive(Debug)]
pub struct Section {
    title: String,
    pub(crate) children: Schema,
    extra_class: Option<String>,
}

impl Section {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            children: Schema::empty(),
            extra_class: None,
        }
    }

    pub fn schema(mut self, children: impl IntoSchema) -> Self {
        self.children = children.into_schema();
        self
    }

    /// Additive `class` hook on the panel container (narrow seam).
    /// Merged via `class!` against the panel classes, never replacing them.
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.extra_class = Some(class.into());
        self
    }

    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let title = self.title.clone();
        let extra = self.extra_class.clone();
        if !self.children.nodes.is_empty() {
            let child_view = render_nodes(cx, &self.children.nodes, fields, source).await?;
            Ok(view! {
                cx =>
                <div class=(class!(PANEL, extra.clone()))>
                    card_header(card_title((title)))
                    card_content(
                        attrs: attributes! { class="flex flex-col gap-6" },
                        (child_view)
                    )
                </div>
            }
            .boxed())
        } else {
            // Header-only on purpose: the panel is `flex flex-col gap-5`, so an
            // empty `card_content` would be a zero-height flex item that still
            // takes a gap slot and adds 20px below the title for nothing.
            // The gap-6 class rides `card_content` only where there
            // are children to space.
            Ok(view! {
                cx =>
                <div class=(class!(PANEL, extra.clone()))>
                    card_header(card_title((title)))
                </div>
            }
            .boxed())
        }
    }
}

/// Group — unlabelled container, useful for grouping fields.
#[derive(Debug, Default)]
pub struct Group {
    pub(crate) children: Schema,
}

impl Group {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn schema(mut self, children: impl IntoSchema) -> Self {
        self.children = children.into_schema();
        self
    }

    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let child_view = render_nodes(cx, &self.children.nodes, fields, source).await?;
        Ok(view! { cx => ui_field_group((child_view)) }.boxed())
    }
}

/// Grid — column container. `cols` is 1..12.
#[derive(Debug)]
pub struct Grid {
    cols: u8,
    pub(crate) children: Schema,
}

impl Grid {
    pub fn new(cols: u8) -> Self {
        Self {
            cols: cols.clamp(1, 12),
            children: Schema::empty(),
        }
    }

    pub fn schema(mut self, children: impl IntoSchema) -> Self {
        self.children = children.into_schema();
        self
    }

    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        // Static literals for Tailwind scanner — `format!("grid grid-cols-{}")` would be
        // purged because Tailwind only sees literal substrings. See ADR-0006.
        let class: &'static str = match self.cols {
            1 => "grid grid-cols-1 gap-4",
            2 => "grid grid-cols-2 gap-4",
            3 => "grid grid-cols-3 gap-4",
            4 => "grid grid-cols-4 gap-4",
            5 => "grid grid-cols-5 gap-4",
            6 => "grid grid-cols-6 gap-4",
            7 => "grid grid-cols-7 gap-4",
            8 => "grid grid-cols-8 gap-4",
            9 => "grid grid-cols-9 gap-4",
            10 => "grid grid-cols-10 gap-4",
            11 => "grid grid-cols-11 gap-4",
            _ => "grid grid-cols-12 gap-4",
        };
        let child_view = render_nodes(cx, &self.children.nodes, fields, source).await?;
        Ok(view! { cx => <div class=(class)>(child_view)</div> }.boxed())
    }
}

/// Repeater — nested Schema repeated as a group (in-memory for v1, no DB array).
///
/// v1 honesty: this is a single-entry group, not a multi-row repeater —
/// one titled panel with its nested schema once, no add/remove UI, no JS, no
/// indexed field names (`tags[0]`). Indexed multi-entry semantics, per-entry
/// validation, and hydration via split/join or a real relation are deferred.
/// `required` means "the inner fields must not all be empty" and its error is
/// keyed by label and rendered inline.
///
/// The panel is `Section`'s panel: one container style for every titled group,
/// with the title inside the box. A `Section` is the titled group; the
/// repeater is only the repeat mechanism. `Group` draws no box.
#[derive(Debug)]
pub struct Repeater {
    pub(crate) label: String,
    pub(crate) children: Schema,
    pub(crate) required: bool,
}

impl Repeater {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            children: Schema::empty(),
            required: false,
        }
    }

    pub fn schema(mut self, children: impl IntoSchema) -> Self {
        self.children = children.into_schema();
        self
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let title = self.label.clone();
        let title_id = repeater_title_id(&self.label);
        let has_children = !self.children.nodes.is_empty();
        // A view renders the group's label over its children's values:
        // a required group is a statement about a submit that cannot happen
        // here, so no `*`, no `aria-invalid`, no error slot.
        if source.mode() == Mode::View {
            let child_view = if has_children {
                Some(render_nodes(cx, &self.children.nodes, fields, source).await?)
            } else {
                None
            };
            return Ok(view! {
                cx =>
                <div
                    class=(class!(PANEL, "ac-field"))
                    role="group"
                    aria-labelledby=(title_id.clone())
                >
                    card_header(
                        card_title(
                            attrs: attributes! { id=(title_id.clone()) },
                            (title)
                        )
                    )
                    if let Some(child_view) = child_view {
                        card_content(
                            attrs: attributes! { class="flex flex-col gap-6" },
                            <div class="grid gap-4">(child_view)</div>
                        )
                    }
                </div>
            }
            .boxed());
        }
        let required = self.required;
        // Own error lives under the label key (see `walk_absent_groups`).
        // Field errors key by field name; repeaters have no field name yet, so the
        // label is the only stable key until repeaters become field-bound.
        // `errors_for` is the one place "view mode has no errors" lives, so a
        // second layout that reads errors cannot forget it.
        let own_error = source.errors_for(&self.label);
        let has_error = own_error.is_some();
        let error_text = own_error.unwrap_or_default().to_string();
        // The group's error is described by the panel, so it needs an id to
        // be referenced by; the label is the key, and a label is not
        // usable as one (ids cannot carry whitespace).
        let error_id = repeater_error_id(&self.label);
        let container_class = if has_error {
            "ac-field ac-field--error"
        } else {
            "ac-field"
        };
        // `card_title` has no invalid state of its own, so the group colors
        // its title when it is invalid.
        let title_class = if has_error { "text-destructive" } else { "" };
        if has_children {
            let child_view = render_nodes(cx, &self.children.nodes, fields, source).await?;
            Ok(view! {
                cx =>
                <div
                    class=(class!(PANEL, container_class))
                    role="group"
                    aria-labelledby=(title_id.clone())
                    data-invalid=(has_error.then_some("true"))
                    aria-invalid=(if has_error { "true" } else { "false" })
                    aria-describedby=(has_error.then_some(error_id.clone()))
                >
                    card_header(
                        card_title(
                            attrs: attributes! { id=(title_id.clone()) class=(title_class) },
                            (title)
                            if required {
                                <span class="text-destructive" aria-hidden="true">
                                    "*"
                                </span>
                            }
                        )
                    )
                    card_content(
                        attrs: attributes! { class="flex flex-col gap-6" },
                        <div class="grid gap-4">(child_view)</div>
                        if has_error {
                            ui_field_error(
                                attrs: attributes! {
                                    id=(error_id.clone())
                                    class="ac-error"
                                    aria-live="polite"
                                },
                                (error_text)
                            )
                        }
                    )
                </div>
            }
            .boxed())
        } else {
            Ok(view! {
                cx =>
                <div
                    class=(class!(PANEL, container_class))
                    role="group"
                    aria-labelledby=(title_id.clone())
                    data-invalid=(has_error.then_some("true"))
                    aria-invalid=(if has_error { "true" } else { "false" })
                    aria-describedby=(has_error.then_some(error_id.clone()))
                >
                    card_header(
                        card_title(
                            attrs: attributes! { id=(title_id.clone()) class=(title_class) },
                            (title)
                            if required {
                                <span class="text-destructive" aria-hidden="true">
                                    "*"
                                </span>
                            }
                        )
                    )
                    if has_error {
                        card_content(
                            attrs: attributes! { class="flex flex-col gap-6" },
                            ui_field_error(
                                attrs: attributes! {
                                    id=(error_id.clone())
                                    class="ac-error"
                                    aria-live="polite"
                                },
                                (error_text)
                            )
                        )
                    }
                </div>
            }
            .boxed())
        }
    }
}

/// The DOM id of a repeater's error node.
///
/// Repeaters are keyed by their label until they become field-bound
/// and an id may not carry the label's whitespace, so the label is
/// slugged: ASCII alphanumerics lowercased, every other run collapsed to one
/// `-`.
fn repeater_error_id(label: &str) -> String {
    format!("{}-error", repeater_slug(label))
}

/// The DOM id of a repeater's title node, labelling the panel as a group.
fn repeater_title_id(label: &str) -> String {
    format!("{}-title", repeater_slug(label))
}

fn repeater_slug(label: &str) -> String {
    let mut slug = String::with_capacity(label.len());
    for character in label.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests;
