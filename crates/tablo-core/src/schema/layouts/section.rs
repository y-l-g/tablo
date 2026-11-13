//! The `Section` block: a titled panel, optionally described, iconed, collapsible or laid out
//! aside its heading.

use tablo_ui::{card_content, card_description, card_header, card_title};
use topcoat::{
    Result,
    context::Cx,
    icon::{IconData, icon},
    view::{StaticClass, class, *},
};

use super::{
    super::{
        Schema,
        condition::Condition,
        fields::Field,
        tree::{IntoSchema, Source, render_nodes},
    },
    Span, spanned,
};

/// Renders a `Section`'s titled panel: a title row, ruled off from the body under it.
const PANEL: StaticClass = class!(
    "flex flex-col rounded-xl border border-border bg-card text-card-foreground shadow-xs",
    "[&>[data-section-title]:not(:last-child)]:border-b [&>[data-section-title]]:border-border",
);

/// Renders a collapsible section's title row: the whole row toggles the body, which an open
/// section rules off under it.
const SUMMARY: StaticClass = class!(
    "flex cursor-pointer list-none items-center gap-2 px-6 py-4 outline-none",
    "focus-visible:ring-[3px] focus-visible:ring-ring/50 [&::-webkit-details-marker]:hidden",
    "[[open]>&]:border-b [[open]>&]:border-border",
);

/// Renders a section with no panel of its own, a rule above it when another block precedes it.
/// Alone in its form the title goes: the dialog around the form already names the record.
const FLAT: StaticClass = class!(
    "flex flex-col gap-4 [&:not(:first-of-type)]:border-t [&:not(:first-of-type)]:border-border",
    "[&:not(:first-of-type)]:pt-6 [&:only-of-type>[data-section-title]]:hidden",
);

/// Renders a section inside another panel: a heading over the fields, a rule above it when a block
/// precedes it in the panel.
const SUBSECTION: StaticClass = class!(
    "flex flex-col gap-4 [&:not(:first-child)]:border-t [&:not(:first-child)]:border-border",
    "[&:not(:first-child)]:pt-6",
);

/// Whether a section folds its body away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Collapse {
    /// The body always shows.
    Never,
    /// The title toggles the body, which the page opens on.
    Open,
    /// The title toggles the body, which the page opens folded unless it holds a field error.
    Closed,
}

/// What a section shows around its body: everything but its children.
#[derive(Debug)]
struct Look {
    title: crate::Text,
    description: Option<crate::Text>,
    icon: Option<IconData>,
    collapse: Collapse,
    aside: bool,
    extra_class: Option<String>,
}

/// A titled block holding a form's fields ([`schema`](Self::schema)) or a detail page's columns
/// ([`columns`](Self::columns), which `detail` declares).
///
/// ```rust
/// # #[derive(Debug, toasty::Model, tablo_core::PanelModel)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String, email: String, bio: String }
/// use tablo_core::{Field, Schema, Section};
///
/// let user = User::fields();
/// Schema::new((
///     Section::new("Account")
///         .description("How the user signs in.")
///         .aside()
///         .schema((Field::text(user.name()), Field::text(user.email()))),
///     Section::new("About")
///         .collapsed()
///         .schema(Field::text(user.bio()).multiline(4)),
/// ));
/// ```
///
/// A collapsible section renders a `<details>` element, so it opens and folds without a script.
/// Its fields render and post while it is folded; a form section holding a field error renders
/// open, and the browser opens a folded section holding a control it refuses on submit. An
/// [`aside`](Self::aside) section does not collapse.
#[derive(Debug)]
pub struct Section<C = Schema> {
    look: Look,
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
    pub(super) span: Option<Span>,
}

impl Section<()> {
    pub fn new(title: impl Into<crate::Text>) -> Self {
        Self {
            look: Look {
                title: title.into(),
                description: None,
                icon: None,
                collapse: Collapse::Never,
                aside: false,
                extra_class: None,
            },
            children: (),
            condition: None,
            span: None,
        }
    }

    /// Holds a form's fields and blocks.
    pub fn schema<F>(self, children: impl IntoSchema<F>) -> Section<Schema<F>> {
        self.holding(children.into_schema())
    }
}

impl<C> Section<C> {
    /// Adds an additive `class` hook on the panel container.
    pub fn class(mut self, class: impl Into<String>) -> Self {
        self.look.extra_class = Some(class.into());
        self
    }

    /// Shows `description` under the title.
    pub fn description(mut self, description: impl Into<crate::Text>) -> Self {
        self.look.description = Some(description.into());
        self
    }

    /// Shows `icon` before the title.
    pub fn icon(mut self, data: IconData) -> Self {
        self.look.icon = Some(data);
        self
    }

    /// Lets the title fold the body away; the page opens on it shown.
    pub fn collapsible(mut self) -> Self {
        self.look.collapse = Collapse::Open;
        self
    }

    /// Lets the title fold the body away, and opens the page on it folded.
    pub fn collapsed(mut self) -> Self {
        self.look.collapse = Collapse::Closed;
        self
    }

    /// Lays the title and description out beside the body rather than over it, on a screen wide
    /// enough for both.
    pub fn aside(mut self) -> Self {
        self.look.aside = true;
        self
    }

    pub(crate) fn holding<D>(self, children: D) -> Section<D> {
        self.map(|_| children)
    }

    pub(crate) fn map<D>(self, f: impl FnOnce(C) -> D) -> Section<D> {
        Section {
            look: self.look,
            children: f(self.children),
            condition: self.condition,
            span: self.span,
        }
    }

    /// The titled panel around `body`, or the title alone when the section holds nothing; inside
    /// another `panel`, the title over `body`.
    pub(crate) fn chrome_in<'a>(
        &self,
        cx: &'a Cx,
        body: Option<BoxView<'a>>,
        panel: bool,
    ) -> BoxView<'a> {
        if panel {
            subsection(cx, &self.look, self.span, body)
        } else {
            section_chrome(cx, &self.look, self.span, body, false)
        }
    }
}

// The chrome is a free function, not a method of the generic block, so tablo-core compiles its
// view once rather than once for every detail model an app declares.

/// The section `look` describes around `body`; `errored` opens a folded section holding a field
/// error.
fn section_chrome<'a>(
    cx: &'a Cx,
    look: &Look,
    span: Option<Span>,
    body: Option<BoxView<'a>>,
    errored: bool,
) -> BoxView<'a> {
    spanned(cx, span, panel(cx, look, body, errored))
}

/// The section's panel, as `section_chrome` spans it.
fn panel<'a>(cx: &'a Cx, look: &Look, body: Option<BoxView<'a>>, errored: bool) -> BoxView<'a> {
    let extra = look.extra_class.clone();
    let heading = heading(cx, look);
    let description = look.description.clone();
    match body {
        Some(body) if look.aside => view! {
            <div
                class=(class!("grid gap-x-8 gap-y-4 md:grid-cols-3", extra.clone()))
                data-section=""
                data-section-aside=""
            >
                <div class="flex flex-col gap-1.5" data-section-title="">
                    (heading)
                    if let Some(description) = description {
                        card_description((description))
                    }
                </div>
                <div class=(class!(PANEL, "md:col-span-2"))>
                    card_content(
                        attrs: attributes! { class="flex flex-col gap-6 py-6" },
                        (body)
                    )
                </div>
            </div>
        }
        .boxed(),
        Some(body) if look.collapse != Collapse::Never => {
            let open = look.collapse == Collapse::Open || errored;
            view! {
                <details
                    class=(class!(PANEL, extra.clone()))
                    data-section=""
                    open=(open)
                >
                    <summary class=(SUMMARY) data-section-title="">
                        <div class="flex min-w-0 flex-1 flex-col gap-1.5">
                            (heading)
                            if let Some(description) = description {
                                card_description((description))
                            }
                        </div>
                        icon(
                            data: tablo_ui::icons::CHEVRON_DOWN,
                            attrs: attributes! {
                                class="shrink-0 text-muted-foreground transition-transform \
                                       [[open]>summary>&]:rotate-180"
                            }
                        )
                    </summary>
                    card_content(
                        attrs: attributes! { class="flex flex-col gap-6 py-6" },
                        (body)
                    )
                </details>
            }
            .boxed()
        }
        body => view! {
            <div class=(class!(PANEL, extra.clone())) data-section="">
                card_header(
                    attrs: attributes! { class="py-4" data-section-title="" },
                    (heading)
                    if let Some(description) = description {
                        card_description((description))
                    }
                )
                if let Some(body) = body {
                    card_content(
                        attrs: attributes! { class="flex flex-col gap-6 py-6" },
                        (body)
                    )
                }
            </div>
        }
        .boxed(),
    }
}

/// The section `look` describes as a heading over `body`, for a form a dialog frames. Nothing folds
/// here: a dialog's form is short, and a section always shows what it asks for.
fn flat<'a>(cx: &'a Cx, look: &Look, span: Option<Span>, body: Option<BoxView<'a>>) -> BoxView<'a> {
    let extra = look.extra_class.clone();
    let heading = heading(cx, look);
    let description = look.description.clone();
    let view = view! {
        <div class=(class!(FLAT, extra)) data-section="">
            <div data-section-title="">(heading)</div>
            if let Some(description) = description {
                card_description((description))
            }
            if let Some(body) = body {
                <div class="flex flex-col gap-6">(body)</div>
            }
        </div>
    }
    .boxed();
    spanned(cx, span, view)
}

/// The section `look` describes inside another panel: its title and description over `body`,
/// ruled off from a block before it. It neither folds nor sits aside: the panel around it already
/// frames the fields.
fn subsection<'a>(
    cx: &'a Cx,
    look: &Look,
    span: Option<Span>,
    body: Option<BoxView<'a>>,
) -> BoxView<'a> {
    let extra = look.extra_class.clone();
    let heading = heading(cx, look);
    let description = look.description.clone();
    let view = view! {
        <div class=(class!(SUBSECTION, extra)) data-section="" data-subsection="">
            <div class="flex flex-col gap-1" data-section-title="">
                (heading)
                if let Some(description) = description {
                    card_description((description))
                }
            </div>
            if let Some(body) = body {
                <div class="flex flex-col gap-6">(body)</div>
            }
        </div>
    }
    .boxed();
    spanned(cx, span, view)
}

/// `body` under `title`, as a section declaring nothing else renders it: a related form's inside
/// its owner's form. Without a title, `body` as it is.
pub(crate) fn titled<'a>(cx: &'a Cx, title: Option<crate::Text>, body: BoxView<'a>) -> BoxView<'a> {
    let Some(title) = title else {
        return body;
    };
    let look = Look {
        title,
        description: None,
        icon: None,
        collapse: Collapse::Never,
        aside: false,
        extra_class: None,
    };
    section_chrome(cx, &look, None, Some(body), false)
}

/// The section's title, after its icon.
fn heading<'a>(cx: &'a Cx, look: &Look) -> BoxView<'a> {
    let title = look.title.clone();
    match look.icon.clone() {
        Some(data) => view! {
            <div class="flex items-center gap-2">
                icon(
                    data: data,
                    attrs: attributes! { class="shrink-0 text-muted-foreground" }
                )
                card_title((title))
            </div>
        }
        .boxed(),
        None => view! { card_title((title)) }.boxed(),
    }
}

impl Section<Schema> {
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        // A section inside another panel is a heading over its fields: a card in a card frames
        // the same fields twice.
        let nested = source.in_panel();
        if self.children.nodes.is_empty() {
            return Ok(if source.is_flat() {
                flat(cx, &self.look, self.span, None)
            } else if nested {
                subsection(cx, &self.look, self.span, None)
            } else {
                section_chrome(cx, &self.look, self.span, None, false)
            });
        }
        let errored = self
            .children
            .nodes
            .iter()
            .any(|node| node.has_error(fields, source));
        let outer = source.set_in_panel(true);
        let body = render_nodes(cx, &self.children.nodes, fields, source).await;
        source.set_in_panel(outer);
        let body = body?;
        if source.is_flat() {
            return Ok(flat(cx, &self.look, self.span, Some(body)));
        }
        if nested {
            return Ok(subsection(cx, &self.look, self.span, Some(body)));
        }
        Ok(section_chrome(
            cx,
            &self.look,
            self.span,
            Some(body),
            errored,
        ))
    }
}
