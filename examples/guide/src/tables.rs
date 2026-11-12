//! The Tables chapter's snippets.

use std::sync::Arc;

use tablo::{
    FilterValue, IntoColumns,
    extend::{Column, Filter, FilterInput},
    prelude::*,
};
use toasty::stmt::Expr;
use topcoat::{context::Cx, view::*};

use crate::{
    models::{Author, Post, PostStatus, Theme, User},
    resources::{AuthorResource, CommentResource},
};

// ANCHOR: post-table-post
pub fn post_table() -> Table<Post> {
    // Each field as the default table shows it: a searchable title, a sortable status.
    let col = Post::columns();
    Table::new((col.title, col.status))
        // ANCHOR: table-filters
        .filters((
            // A checklist: the posts holding any checked status.
            SelectFilter::of(lens!(Post.status)).multiple(),
            TernaryFilter::new(lens!(Post.featured)).labels("Featured", "Not featured"),
            // Two date inputs: the first and the last day.
            DateFilter::new(lens!(Post.created_at)).range(),
            // The authors `AuthorResource` lists, by name.
            SelectFilter::relationship::<AuthorResource>(lens!(Post.author_id)).label("Author"),
        ))
        // ANCHOR_END: table-filters
        // ANCHOR: table-tabs
        .tab(ListTab::new("all", "All"))
        .tab(
            ListTab::new("drafts", "Drafts")
                .query(Post::fields().status().eq(PostStatus::Draft))
                .badge(),
        )
        // ANCHOR_END: table-tabs
        // ANCHOR: table-summaries
        .summary(Summary::count("Posts"))
        .summary(Summary::count("Featured").query(Post::fields().featured().eq(true)))
        // ANCHOR_END: table-summaries
        // ANCHOR: table-group-by
        .group_by(lens!(Post.status))
        .group_by(lens!(Post.featured))
    // ANCHOR_END: table-group-by
}
// ANCHOR_END: post-table-post

// ANCHOR: table-comparison-filters
pub fn filtered_user_table() -> Table<User> {
    User::table().filters((
        // Two number inputs: `?f.age.min=18&f.age.max=65`, both ends included.
        NumberFilter::new(lens!(User.age)).range(),
        // A term and how it matches: `?f.email=example.com&f.email.op=ends`.
        TextFilter::new(lens!(User.email)).matching([TextMatch::EndsWith, TextMatch::Is]),
    ))
}
// ANCHOR_END: table-comparison-filters

// ANCHOR: table-option-tabs
pub fn status_tabs() -> Table<Post> {
    Table::new(Post::columns().title).option_tabs(lens!(Post.status))
}
// ANCHOR_END: table-option-tabs

// ANCHOR: table-date
pub fn date_column() -> DateColumn<User, jiff::Timestamp> {
    User::columns().created_at.date().sortable()
}
// ANCHOR_END: table-date

// ANCHOR: table-presenting
pub fn user_columns() -> Table<User> {
    let col = User::columns();
    Table::new((
        // The email as a muted line under the name.
        col.name
            .text()
            .description(|_, user: &User| user.email.clone()),
        // `12,345`, right-aligned.
        col.age.text().numeric(0),
        // `$1,234.50`, from a field holding cents.
        ComputedColumn::new("Balance", |_, user: &User| {
            Currency::USD.minor_units().format(user.age * 1250)
        })
        .align(Alignment::End),
    ))
}

pub fn status_badge() -> TextColumn<Post, crate::models::PostStatus> {
    // A dot in the variant's `#[option(color = "…")]`.
    Post::columns().status.text().badge()
}

pub fn accent_column() -> ColorColumn<Theme, String> {
    Theme::columns().accent.color()
}
// ANCHOR_END: table-presenting

// ANCHOR: table-cell-options
pub fn contact_table() -> Table<User> {
    let col = User::columns();
    Table::new((
        // Links to the user's page on another site, in a new tab.
        col.name
            .text()
            .url(|_, user: &User| Some(format!("https://people.example/{}", user.id)))
            .open_url_in_new_tab(),
        // A button copies the address; a long one shows its first 24 characters.
        col.email.text().copyable().limit(24),
        // The role's `#[option(icon = …)]` in place of its label.
        col.role.icon(),
    ))
    // A row whose user is inactive reads muted.
    .row_color(|_, user: &User| (!user.active).then_some(Color::Gray))
    .row_color_reading(Reads::of(lens!(User.active)))
}
// ANCHOR_END: table-cell-options

// ANCHOR: table-editable
pub fn triage_table() -> Table<Post> {
    let col = Post::columns();
    Table::new((
        // Saves on Enter.
        col.title.text_input(),
        // Each saves as soon as it changes.
        col.status.select(),
        col.featured.toggle(),
    ))
}
// ANCHOR_END: table-editable

// ANCHOR: table-toggleable
pub fn user_table() -> Table<User> {
    let col = User::columns();
    Table::new((
        col.name,
        // Shown until a reader hides it.
        col.email.text().toggleable(),
        // Hidden until a reader shows it.
        col.created_at.date().hidden_by_default(),
    ))
}
// ANCHOR_END: table-toggleable

// ANCHOR: table-derived-keys
#[derive(Debug, Clone, toasty::Model, tablo::PanelModel)]
pub struct Invoice {
    #[key]
    #[auto]
    pub id: uuid::Uuid,
    // A badge in the variant's color, and a select filter over the variants.
    #[columns(badge, filter)]
    pub status: crate::models::PostStatus,
    // `€1,234.00`, labelled "Amount due".
    #[columns(label = "Amount due", money = EUR)]
    pub total: i64,
    // Hidden until a reader shows it, and a date range filter.
    #[columns(hidden_by_default, filter)]
    pub issued_at: jiff::Timestamp,
}
// ANCHOR_END: table-derived-keys

// ANCHOR: table-relation-column
pub fn author_column() -> RelationColumn<Post> {
    RelationColumn::new::<AuthorResource>(
        relation!(Post.author),
        lens!(Author.name),
        |_, a: &Author| a.name.clone(),
    )
}

pub fn comment_count_column() -> CountColumn<Post> {
    CountColumn::of::<CommentResource>(relation!(Post.comments))
}
// ANCHOR_END: table-relation-column

// ANCHOR: table-custom-column
pub struct Initials;

impl Column<User> for Initials {
    fn name(&self) -> &str {
        "initials"
    }
    fn label(&self) -> tablo::Text {
        "Initials".into()
    }

    // The export's cell, and the table's unless `cell` renders a view.
    fn text(&self, _cx: &Cx, u: &User) -> String {
        u.name
            .split_whitespace()
            .filter_map(|w| w.chars().next())
            .collect()
    }

    fn cell<'a>(&self, cx: &'a Cx, u: &User) -> BoxView<'a> {
        let text = self.text(cx, u);
        view! { <span class="font-mono">(text)</span> }.boxed()
    }
}

// Joins a table's tuple, as a built-in column does.
impl IntoColumns<User> for Initials {
    fn into_columns(self) -> Vec<Arc<dyn Column<User>>> {
        vec![Arc::new(self)]
    }
}
// ANCHOR_END: table-custom-column

// ANCHOR: table-promoted-filter
pub fn promoted_filter() -> QueryFilter<Post> {
    QueryFilter::new("promoted", "Promoted")
        .option("Promoted", Post::fields().featured().eq(true))
        // `?f.promoted=backlog`, whatever the label reads.
        .option_keyed(
            "backlog",
            "In the backlog",
            Post::fields().featured().eq(false),
        )
}
// ANCHOR_END: table-promoted-filter

// ANCHOR: table-adults-filter
pub struct Adults;

impl Filter<User> for Adults {
    fn name(&self) -> &str {
        "adults"
    }
    fn label(&self) -> tablo::Text {
        "Adults".into()
    }
    fn to_expr(&self, _cx: &Cx, value: &FilterValue) -> Option<Expr<bool>> {
        (value.single() == Some("yes")).then(|| User::fields().age().ge(18))
    }
    fn control<'a>(&self, cx: &'a Cx, input: FilterInput) -> BoxView<'a> {
        input.select(cx, vec![("yes".into(), "Adults only".into())])
    }
}
// ANCHOR_END: table-adults-filter
