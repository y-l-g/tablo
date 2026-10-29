//! Holds the `Section`, `Group`, `Grid` and `Tabs` blocks that lay out a form's fields or a detail
//! page's columns.

use tablo_ui::field_group as ui_field_group;
use topcoat::{
    Result,
    context::Cx,
    view::{StaticClass, class, *},
};

use super::{
    Schema,
    condition::{Condition, Watched},
    fields::Field,
    tree::{IntoSchema, Node, Retype, Source, render_nodes},
};

mod section;

pub use section::Section;
pub(crate) use section::titled;

/// An unlabelled block grouping a form's fields or a detail page's columns.
#[derive(Debug)]
pub struct Group<C = Schema> {
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
}

impl Default for Group<()> {
    fn default() -> Self {
        Self {
            children: (),
            condition: None,
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
        }
    }

    /// The group around `body`.
    pub(crate) fn chrome<'a>(&self, cx: &'a Cx, body: BoxView<'a>) -> BoxView<'a> {
        group_chrome(cx, body)
    }
}

fn group_chrome<'a>(cx: &'a Cx, body: BoxView<'a>) -> BoxView<'a> {
    view! { cx => ui_field_group((body)) }.boxed()
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
#[derive(Debug)]
pub struct Grid<C = Schema> {
    cols: u8,
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
}

impl Grid<()> {
    pub fn new(cols: u8) -> Self {
        Self {
            cols: cols.clamp(1, 12),
            children: (),
            condition: None,
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
        }
    }

    /// The grid around `body`.
    pub(crate) fn chrome<'a>(&self, cx: &'a Cx, body: BoxView<'a>) -> BoxView<'a> {
        grid_chrome(cx, self.cols, body)
    }
}

fn grid_chrome<'a>(cx: &'a Cx, cols: u8, body: BoxView<'a>) -> BoxView<'a> {
    // Static literals for Tailwind scanner — `format!("grid grid-cols-{}")` would be
    // purged because Tailwind only sees literal substrings.
    let class: &'static str = match cols {
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
    view! { cx => <div class=(class)>(body)</div> }.boxed()
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

conditional_blocks!(Section, Group, Grid, Tabs, Tab);

/// Renders a `Tabs` block's list of triggers.
const TAB_LIST: StaticClass = class!(
    "inline-flex h-9 w-fit max-w-full items-center overflow-x-auto rounded-lg bg-muted p-[3px] \
     text-muted-foreground"
);
/// Renders one trigger, which the selected tab raises.
const TAB_TRIGGER: StaticClass = class!(
    "inline-flex h-[calc(100%-1px)] items-center justify-center gap-1.5 rounded-md border \
     border-transparent px-3 py-1 text-sm font-medium whitespace-nowrap text-foreground \
     transition-[color,box-shadow] outline-none focus-visible:border-ring focus-visible:ring-[3px] \
     focus-visible:ring-ring/50 aria-selected:bg-background aria-selected:shadow-sm \
     dark:text-muted-foreground dark:aria-selected:border-input dark:aria-selected:bg-input/30 \
     dark:aria-selected:text-foreground data-tab-error:text-destructive"
);

/// A block showing one [`Tab`] at a time, each holding a form's fields
/// ([`Tab::schema`]) or a detail page's columns ([`Tab::columns`]).
///
/// ```rust
/// # #[derive(Debug, toasty::Model, tablo_core::ModelColumns)]
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
        }
    }
}

impl<C> Tabs<C> {
    pub(crate) fn map<D>(self, f: impl FnOnce(C) -> D) -> Tabs<D> {
        Tabs {
            children: f(self.children),
            condition: self.condition,
        }
    }
}

/// One tab of a [`Tabs`] block: a titled trigger and the fields or columns it shows.
#[derive(Debug)]
pub struct Tab<C = Schema> {
    label: crate::Text,
    pub(crate) children: C,
    pub(crate) condition: Option<Condition>,
}

impl Tab<()> {
    pub fn new(label: impl Into<crate::Text>) -> Self {
        Self {
            label: label.into(),
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
            label: self.label,
            children: f(self.children),
            condition: self.condition,
        }
    }

    /// The trigger's text.
    pub(crate) fn label(&self) -> &crate::Text {
        &self.label
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

/// Renders the tab `id` of a block whose active tab is `active`: a trigger reading `label`, and a
/// panel holding `body`, hidden unless `active`. `errored` marks the trigger of a tab holding a
/// field error.
pub(crate) fn tab_view<'a>(
    cx: &'a Cx,
    id: &str,
    label: &crate::Text,
    active: bool,
    errored: bool,
    body: BoxView<'a>,
) -> TabView<'a> {
    let tab = format!("{id}-tab");
    let panel = format!("{id}-tabpanel");
    let (labelled_by, panel_id) = (tab.clone(), panel.clone());
    let label = label.localize(cx);
    let selected = if active { "true" } else { "false" };
    let tabindex = if active { "0" } else { "-1" };
    let trigger = view! {
        cx =>
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
            (label)
        </button>
    }
    .boxed();
    let panel = view! {
        cx =>
        <div
            role="tabpanel"
            id=(panel_id)
            aria-labelledby=(labelled_by)
            tabindex="0"
            hidden=(!active)
            data-tab-panel=""
            class="flex flex-col gap-6 outline-none"
        >
            (body)
        </div>
    }
    .boxed();
    TabView { trigger, panel }
}

/// The block around its rendered `tabs`: the triggers in a list over the panels.
pub(crate) fn tabs_chrome<'a>(cx: &'a Cx, tabs: Vec<TabView<'a>>) -> BoxView<'a> {
    let (triggers, panels): (Vec<_>, Vec<_>) =
        tabs.into_iter().map(|tab| (tab.trigger, tab.panel)).unzip();
    view! {
        cx =>
        <div data-tabs="" class="flex flex-col gap-4">
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
            let body = render_nodes(cx, &tab.children.nodes, fields, source).await;
            source.set_tab_depth(depth);
            let body = body?;
            let view = tab_view(cx, &id, tab.label(), index == active, errored[index], body);
            views.push(TabView {
                trigger: source.guard(cx, tab.condition.as_ref(), view.trigger),
                panel: source.guard(cx, tab.condition.as_ref(), view.panel),
            });
        }
        Ok(tabs_chrome(cx, views))
    }
}
