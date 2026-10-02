//! Holds the `Section`, `Group`, `Grid`, and `Repeater` containers that compose form layout.

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

/// Renders the one titled-group panel that `Section` and `Repeater` share.
const PANEL: StaticClass = class!(
    "flex flex-col gap-5 rounded-xl border border-border bg-card py-6 text-card-foreground shadow-sm"
);

/// Holds a titled container with an optional child `Schema`.
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

    /// Adds an additive `class` hook on the panel container.
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

/// Holds an unlabelled container for grouping fields.
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

/// Holds a column container with `cols` clamped to 1..12.
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

/// Holds a nested `Schema` rendered once as a titled group and reports a `required` empty group
/// under its label.
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
        let own_error = source.errors_for(&self.label);
        let has_error = own_error.is_some();
        let error_text = own_error.unwrap_or_default().to_string();
        let error_id = repeater_error_id(&self.label);
        let container_class = if has_error {
            "ac-field ac-field--error"
        } else {
            "ac-field"
        };
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

/// Returns the DOM id of a repeater's error node, slugging the label.
fn repeater_error_id(label: &str) -> String {
    format!("{}-error", repeater_slug(label))
}

/// Returns the DOM id of a repeater's title node.
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
