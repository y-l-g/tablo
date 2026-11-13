//! Holds the `Section`, `Group`, `Grid`, `Tabs` and `Wizard` blocks that lay out a form's fields or
//! a detail page's columns.

use tablo_ui::field_group as ui_field_group;
use topcoat::{
    Result,
    context::Cx,
    icon::{IconData, icon},
    view::{StaticClass, class, *},
};

use super::{
    super::Color,
    Schema,
    condition::{Condition, Watched},
    fields::Field,
    tree::{IntoSchema, Node, Retype, Source, render_nodes},
};

mod section;
mod span;
mod wizard;

pub use section::Section;
pub(crate) use section::titled;
pub(crate) use span::{Span, spanned};
pub use wizard::{Step, Wizard};

/// An unlabelled block grouping a form's fields or a detail page's columns.
#[derive(Debug)]
pub struct Group<C = Schema> {
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
    span: Option<Span>,
}

impl Default for Group<()> {
    fn default() -> Self {
        Self {
            children: (),
            condition: None,
            span: None,
        }
    }
}

impl Group<()> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Holds a form's fields and blocks.
    pub fn schema<F>(self, children: impl IntoSchema<F>) -> Group<Schema<F>> {
        self.holding(children.into_schema())
    }
}

impl<C> Group<C> {
    pub(crate) fn holding<D>(self, children: D) -> Group<D> {
        self.map(|_| children)
    }

    pub(crate) fn map<D>(self, f: impl FnOnce(C) -> D) -> Group<D> {
        Group {
            children: f(self.children),
            condition: self.condition,
            span: self.span,
        }
    }

    /// The group around `body`.
    pub(crate) fn chrome<'a>(&self, cx: &'a Cx, body: BoxView<'a>) -> BoxView<'a> {
        group_chrome(cx, self.span, body)
    }
}

fn group_chrome<'a>(cx: &'a Cx, span: Option<Span>, body: BoxView<'a>) -> BoxView<'a> {
    spanned(cx, span, view! { ui_field_group((body)) }.boxed())
}

impl Group<Schema> {
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let body = render_nodes(cx, &self.children.nodes, fields, source).await?;
        Ok(self.chrome(cx, body))
    }
}

/// A block of `cols` columns, clamped to 1..12, laying out a form's fields or a detail page's
/// columns.
///
/// The grid lays its columns out once it is wide enough for them: 28rem for two columns, 42rem for
/// three and 56rem for four or more. Narrower, it stacks its children in one column. It measures
/// its own width rather than the screen's, so a grid in a narrow column stacks on a wide screen. A
/// child spans more than one column through its `span`.
///
/// ```rust
/// # #[derive(Debug, toasty::Model, tablo_core::PanelModel)]
/// # struct Product {
/// #     #[key] #[auto] id: uuid::Uuid, name: String, description: String, visible: bool,
/// # }
/// use tablo_core::{Field, Grid, Group, Schema, Section};
///
/// let product = Product::fields();
/// Schema::new(Grid::new(3).schema((
///     Group::new().span(2).schema(Section::new("Product").schema((
///         Field::text(product.name()),
///         Field::text(product.description()).multiline(4),
///     ))),
///     Section::new("Status").schema(Field::toggle(product.visible())),
/// )));
/// ```
#[derive(Debug)]
pub struct Grid<C = Schema> {
    cols: u8,
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
    span: Option<Span>,
}

impl Grid<()> {
    pub fn new(cols: u8) -> Self {
        Self {
            cols: cols.clamp(1, 12),
            children: (),
            condition: None,
            span: None,
        }
    }

    /// Holds a form's fields and blocks.
    pub fn schema<F>(self, children: impl IntoSchema<F>) -> Grid<Schema<F>> {
        self.holding(children.into_schema())
    }
}

impl<C> Grid<C> {
    pub(crate) fn holding<D>(self, children: D) -> Grid<D> {
        self.map(|_| children)
    }

    pub(crate) fn map<D>(self, f: impl FnOnce(C) -> D) -> Grid<D> {
        Grid {
            cols: self.cols,
            children: f(self.children),
            condition: self.condition,
            span: self.span,
        }
    }

    /// The grid around `body`.
    pub(crate) fn chrome<'a>(&self, cx: &'a Cx, body: BoxView<'a>) -> BoxView<'a> {
        grid_chrome(cx, self.cols, self.span, body)
    }
}

fn grid_chrome<'a>(cx: &'a Cx, cols: u8, span: Option<Span>, body: BoxView<'a>) -> BoxView<'a> {
    // Static literals for Tailwind scanner — `format!("grid grid-cols-{}")` would be
    // purged because Tailwind only sees literal substrings. `--tablo-cols` tells a spanning
    // child how many columns the grid lays out at its current width.
    let class: &'static str = match cols {
        1 => "grid grid-cols-1 gap-4 [--tablo-cols:1]",
        2 => "grid grid-cols-1 gap-4 [--tablo-cols:1] @md:grid-cols-2 @md:[--tablo-cols:2]",
        3 => "grid grid-cols-1 gap-4 [--tablo-cols:1] @2xl:grid-cols-3 @2xl:[--tablo-cols:3]",
        4 => "grid grid-cols-1 gap-4 [--tablo-cols:1] @4xl:grid-cols-4 @4xl:[--tablo-cols:4]",
        5 => "grid grid-cols-1 gap-4 [--tablo-cols:1] @4xl:grid-cols-5 @4xl:[--tablo-cols:5]",
        6 => "grid grid-cols-1 gap-4 [--tablo-cols:1] @4xl:grid-cols-6 @4xl:[--tablo-cols:6]",
        7 => "grid grid-cols-1 gap-4 [--tablo-cols:1] @4xl:grid-cols-7 @4xl:[--tablo-cols:7]",
        8 => "grid grid-cols-1 gap-4 [--tablo-cols:1] @4xl:grid-cols-8 @4xl:[--tablo-cols:8]",
        9 => "grid grid-cols-1 gap-4 [--tablo-cols:1] @4xl:grid-cols-9 @4xl:[--tablo-cols:9]",
        10 => "grid grid-cols-1 gap-4 [--tablo-cols:1] @4xl:grid-cols-10 @4xl:[--tablo-cols:10]",
        11 => "grid grid-cols-1 gap-4 [--tablo-cols:1] @4xl:grid-cols-11 @4xl:[--tablo-cols:11]",
        _ => "grid grid-cols-1 gap-4 [--tablo-cols:1] @4xl:grid-cols-12 @4xl:[--tablo-cols:12]",
    };
    // The grid measures the container around it: an element cannot query its own width.
    let grid = view! { <div class="@container"><div class=(class)>(body)</div></div> }.boxed();
    spanned(cx, span, grid)
}

impl Grid<Schema> {
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let body = render_nodes(cx, &self.children.nodes, fields, source).await?;
        Ok(self.chrome(cx, body))
    }
}

/// Generates `visible_when` for every layout block holding a form's fields.
macro_rules! conditional_blocks {
    ($($ty:ident),+ $(,)?) => {
        $(
            impl<F> $ty<Schema<F>> {
                /// Shows the block and every field it holds only while `watched` posts one of
                /// `values`, as [`TextField::visible_when`](crate::TextField::visible_when) shows a
                /// field.
                pub fn visible_when(
                    mut self,
                    watched: &impl Watched<F>,
                    values: impl IntoIterator<Item = impl Into<String>>,
                ) -> Self {
                    self.condition = Some(Condition::new(watched, values));
                    self
                }
            }
        )+
    };
}

conditional_blocks!(Section, Group, Grid, Tabs, Tab, Wizard, Step);

/// Generates `span` and `span_full` for every block a [`Grid`] lays out.
macro_rules! spanning_blocks {
    ($($ty:ident),+ $(,)?) => {
        $(
            impl<C> $ty<C> {
                /// Spans `columns` of the [`Grid`] holding the block, clamped to 1..12. While the
                /// grid stacks its children, or lays out fewer columns, the block spans them all.
                /// Outside a grid, the span does nothing.
                pub fn span(mut self, columns: u8) -> Self {
                    self.span = Some(Span::columns(columns));
                    self
                }

                /// Spans every column of the [`Grid`] holding the block.
                pub fn span_full(mut self) -> Self {
                    self.span = Some(Span::Full);
                    self
                }
            }
        )+
    };
}

spanning_blocks!(Section, Group, Grid, Tabs, Wizard);

/// Renders a `Tabs` block: a card whose header is the list of triggers, the way a list page shows
/// its tabs. Inside a section's card, or the dialog around a framed form, it draws no card of its
/// own and its tabs sit flush with the fields around them.
const TABS: StaticClass = class!(
    "flex min-w-0 flex-col rounded-xl border border-border bg-card text-card-foreground shadow-xs",
    "in-[[data-section],[data-pick-page]]:rounded-none in-[[data-section],[data-pick-page]]:border-0",
    "in-[[data-section],[data-pick-page]]:bg-transparent in-[[data-section],[data-pick-page]]:shadow-none",
);
/// Renders a `Tabs` block's list of triggers: a rule the selected tab's underline covers, which
/// scrolls sideways without a scrollbar when the tabs outgrow it.
const TAB_LIST: StaticClass = class!(
    "flex max-w-full items-stretch gap-1 overflow-x-auto overflow-y-hidden border-b border-border",
    "px-3 [scrollbar-width:none] sm:px-4 [&::-webkit-scrollbar]:hidden",
    "in-[[data-section],[data-pick-page]]:px-0",
);
/// Renders one trigger, which the selected tab underlines.
const TAB_TRIGGER: StaticClass = class!(
    "-mb-px inline-flex h-11 shrink-0 items-center gap-2 border-b-2 border-transparent px-2.5",
    "text-sm font-medium whitespace-nowrap text-muted-foreground transition-colors outline-none",
    "hover:text-foreground focus-visible:text-foreground focus-visible:ring-2",
    "focus-visible:ring-ring/50 focus-visible:ring-inset aria-selected:border-primary",
    "aria-selected:text-foreground data-tab-error:text-destructive",
    "data-tab-error:aria-selected:border-destructive",
);
/// Renders the panel of one tab, padded as a card's content unless the block draws no card.
const TAB_PANEL: StaticClass = class!(
    "flex flex-col gap-6 p-4 outline-none sm:p-6",
    "in-[[data-section],[data-pick-page]]:px-0 in-[[data-section],[data-pick-page]]:pb-0",
);

/// A block showing one [`Tab`] at a time, each holding a form's fields
/// ([`Tab::schema`]) or a detail page's columns ([`Tab::columns`]).
///
/// ```rust
/// # #[derive(Debug, toasty::Model, tablo_core::PanelModel)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String, email: String, bio: String }
/// use tablo_core::{Field, Schema, Tab, Tabs};
///
/// let user = User::fields();
/// Schema::new(Tabs::new([
///     Tab::new("Account").schema((Field::text(user.name()), Field::text(user.email()))),
///     Tab::new("About").schema(Field::text(user.bio()).multiline(4)),
/// ]));
/// ```
///
/// Every tab's fields render and post: an inactive tab's are only hidden, so a submission carries
/// them as if no tab held them, and the browser's own checks still refuse what they refuse. The
/// page opens on the first tab holding a field error, else on the first tab; the browser opens
/// the tab of a control it refuses on submit. A tab whose every field or column the request may
/// not see renders no trigger, and a block with no such tab renders nothing.
#[derive(Debug)]
pub struct Tabs<C = Schema> {
    /// The block's tabs, each one node.
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
    span: Option<Span>,
}

impl<C: HoldsTabs> Tabs<C> {
    /// Holds `tabs`, in the order their triggers show.
    pub fn new(tabs: impl IntoIterator<Item = Tab<C>>) -> Self {
        let mut children = C::default();
        for tab in tabs {
            children.push_tab(tab);
        }
        Self {
            children,
            condition: None,
            span: None,
        }
    }
}

impl<C> Tabs<C> {
    pub(crate) fn map<D>(self, f: impl FnOnce(C) -> D) -> Tabs<D> {
        Tabs {
            children: f(self.children),
            condition: self.condition,
            span: self.span,
        }
    }

    /// The block around its rendered `tabs`.
    pub(crate) fn chrome<'a>(&self, cx: &'a Cx, tabs: Vec<TabView<'a>>) -> BoxView<'a> {
        spanned(cx, self.span, tabs_chrome(cx, tabs))
    }
}

/// One tab of a [`Tabs`] block: a titled trigger and the fields or columns it shows.
///
/// ```rust
/// # #[derive(Debug, toasty::Model, tablo_core::PanelModel)]
/// # struct User { #[key] #[auto] id: uuid::Uuid, name: String, bio: String }
/// use tablo_core::{Color, Field, Tab};
///
/// let user = User::fields();
/// Tab::new("About")
///     .icon(tablo_ui::icons::USER)
///     .badge("New")
///     .badge_color(Color::Info)
///     .schema(Field::text(user.bio()).multiline(4));
/// ```
#[derive(Debug)]
pub struct Tab<C = Schema> {
    face: TabFace,
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
}

/// What a tab's trigger shows.
#[derive(Debug)]
pub(crate) struct TabFace {
    label: crate::Text,
    icon: Option<IconData>,
    badge: Option<crate::Text>,
    badge_color: Option<Color>,
}

impl Tab<()> {
    pub fn new(label: impl Into<crate::Text>) -> Self {
        Self {
            face: TabFace {
                label: label.into(),
                icon: None,
                badge: None,
                badge_color: None,
            },
            children: (),
            condition: None,
        }
    }

    /// Holds a form's fields and blocks.
    pub fn schema<F>(self, children: impl IntoSchema<F>) -> Tab<Schema<F>> {
        self.holding(children.into_schema())
    }
}

impl<C> Tab<C> {
    pub(crate) fn holding<D>(self, children: D) -> Tab<D> {
        self.map(|_| children)
    }

    pub(crate) fn map<D>(self, f: impl FnOnce(C) -> D) -> Tab<D> {
        Tab {
            face: self.face,
            children: f(self.children),
            condition: self.condition,
        }
    }

    /// Shows `icon` before the label.
    pub fn icon(mut self, data: IconData) -> Self {
        self.face.icon = Some(data);
        self
    }

    /// Shows a badge reading `text` after the label, such as a count or `"New"`.
    pub fn badge(mut self, text: impl Into<crate::Text>) -> Self {
        self.face.badge = Some(text.into());
        self
    }

    /// Leads the badge with a dot in `color`.
    pub fn badge_color(mut self, color: Color) -> Self {
        self.face.badge_color = Some(color);
        self
    }

    /// What the trigger shows.
    pub(crate) fn face(&self) -> &TabFace {
        &self.face
    }
}

/// What a [`Tabs`] block holds its tabs in: a form's [`Schema`] or a detail page's
/// [`Detail`](crate::Detail).
#[doc(hidden)]
pub trait HoldsTabs: Default + Sized {
    fn push_tab(&mut self, tab: Tab<Self>);
}

impl<F> HoldsTabs for Schema<F> {
    fn push_tab(&mut self, tab: Tab<Self>) {
        let mut fields = Vec::new();
        let tab = tab.map(|mut children| {
            fields = std::mem::take(&mut children.fields);
            children.retype()
        });
        self.append(Schema::from_parts(vec![Node::Tab(Box::new(tab))], fields));
    }
}

/// The DOM id prefix of the tab whose first field or column renders under `id`, inside `depth`
/// tabs: an outer tab and the first tab of a block it holds start at the same field.
pub(crate) fn tab_id(id: &str, depth: usize) -> String {
    format!("{id}-{depth}")
}

/// One tab of a [`Tabs`] block, rendered: its trigger and its panel apart, so a condition on the
/// tab guards each.
pub(crate) struct TabView<'a> {
    pub(crate) trigger: BoxView<'a>,
    pub(crate) panel: BoxView<'a>,
}

/// Renders the tab `id` of a block whose active tab is `active`: a trigger showing `face`, and a
/// panel holding `body`, hidden unless `active`. `errored` marks the trigger of a tab holding a
/// field error.
pub(crate) fn tab_view<'a>(
    cx: &'a Cx,
    id: &str,
    face: &TabFace,
    active: bool,
    errored: bool,
    body: BoxView<'a>,
) -> TabView<'a> {
    let tab = format!("{id}-tab");
    let panel = format!("{id}-tabpanel");
    let (labelled_by, panel_id) = (tab.clone(), panel.clone());
    let label = face.label.localize(cx);
    let glyph = face.icon.clone();
    let badge = face.badge.as_ref().map(|text| text.localize(cx));
    let color = face.badge_color;
    let selected = if active { "true" } else { "false" };
    let tabindex = if active { "0" } else { "-1" };
    let trigger = view! {
        <button
            type="button"
            role="tab"
            id=(tab)
            aria-controls=(panel)
            aria-selected=(selected)
            tabindex=(tabindex)
            data-tab-error=(errored)
            class=(TAB_TRIGGER)
        >
            if let Some(data) = glyph {
                icon(
                    data: data,
                    attrs: attributes! { class="size-4 shrink-0" aria-hidden="true" }
                )
            }
            (label)
            if let Some(text) = badge {
                <span class="contents tabular-nums" data-tab-badge="">
                    (crate::display::badge(cx, text, color, None))
                </span>
            }
        </button>
    }
    .boxed();
    let panel = view! {
        <div
            role="tabpanel"
            id=(panel_id)
            aria-labelledby=(labelled_by)
            tabindex="0"
            hidden=(!active)
            data-tab-panel=""
            class=(TAB_PANEL)
        >
            (body)
        </div>
    }
    .boxed();
    TabView { trigger, panel }
}

/// The block around its rendered `tabs`: the triggers in a list over the panels.
fn tabs_chrome<'a>(cx: &'a Cx, tabs: Vec<TabView<'a>>) -> BoxView<'a> {
    let (triggers, panels): (Vec<_>, Vec<_>) =
        tabs.into_iter().map(|tab| (tab.trigger, tab.panel)).unzip();
    view! {
        <div data-tabs="" class=(TABS)>
            <div role="tablist" class=(TAB_LIST)>
                for trigger in triggers {
                    (trigger)
                }
            </div>
            for panel in panels {
                (panel)
            }
        </div>
    }
    .boxed()
}

impl Tabs<Schema> {
    pub(crate) async fn render<'a>(
        &self,
        cx: &'a Cx,
        fields: &[Field],
        source: &Source<'_>,
    ) -> Result<BoxView<'a>> {
        let tabs: Vec<(&Node, &Tab)> = self
            .children
            .nodes
            .iter()
            .filter_map(|node| match node {
                Node::Tab(tab) if node.shows_any(fields, source) => Some((node, &**tab)),
                _ => None,
            })
            .collect();
        let errored: Vec<bool> = tabs
            .iter()
            .map(|(node, _)| node.has_error(fields, source))
            .collect();
        // The first tab holding an error, else the first the page shows.
        let active = errored.iter().position(|e| *e).unwrap_or_else(|| {
            tabs.iter()
                .position(|(_, tab)| source.holds(tab.condition.as_ref()))
                .unwrap_or(0)
        });
        let depth = source.tab_depth();
        let mut views = Vec::with_capacity(tabs.len());
        for (index, (node, tab)) in tabs.iter().enumerate() {
            let id = node
                .first_field()
                .map(|field| tab_id(&source.id(fields[field].name()), depth))
                .unwrap_or_default();
            source.set_tab_depth(depth + 1);
            // The block is a card, or sits flush in one: either way its sections are headings.
            let outer = source.set_in_panel(true);
            let body = render_nodes(cx, &tab.children.nodes, fields, source).await;
            source.set_in_panel(outer);
            source.set_tab_depth(depth);
            let body = body?;
            let view = tab_view(cx, &id, tab.face(), index == active, errored[index], body);
            views.push(TabView {
                trigger: source.guard(cx, tab.condition.as_ref(), view.trigger),
                panel: source.guard(cx, tab.condition.as_ref(), view.panel),
            });
        }
        Ok(self.chrome(cx, views))
    }
}
