//! The Tables chapter's snippets.

use tablo::{
    FilterValue,
    extend::{Column, Filter, FilterInput},
    prelude::*,
};
use toasty::stmt::Expr;
use topcoat::{context::Cx, view::*};

use crate::{
    models::{Author, Post, Theme, User},
    resources::{AuthorResource, CommentResource},
};

// ANCHOR: post-table-post
pub fn post_table() -> Table<Post> {
    Table::new((
        TextColumn::new(lens!(Post.title)),
        TextColumn::new(lens!(Post.status)),
    ))
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
    // ANCHOR: table-group-by
    .group_by(lens!(Post.status))
    // ANCHOR_END: table-group-by
}
// ANCHOR_END: post-table-post

// ANCHOR: table-date
pub fn date_column() -> DateColumn<User, jiff::Timestamp> {
    DateColumn::new(lens!(User.created_at)).sortable()
}
// ANCHOR_END: table-date

// ANCHOR: table-presenting
pub fn user_columns() -> Table<User> {
    Table::new((
        // The email as a muted line under the name.
        TextColumn::new(lens!(User.name)).description(|user: &User| user.email.clone()),
        // `12,345`, right-aligned.
        TextColumn::new(lens!(User.age)).numeric(0),
        // `$1,234.50`, from a field holding cents.
        ComputedColumn::new("Balance", |user: &User| {
            Currency::USD.minor_units().format(user.age * 1250)
        })
        .align(Alignment::End),
    ))
}

pub fn status_badge() -> TextColumn<Post, crate::models::PostStatus> {
    // A dot in the variant's `#[option(color = "…")]`.
    TextColumn::new(lens!(Post.status)).badge()
}

pub fn accent_column() -> ColorColumn<Theme, String> {
    ColorColumn::new(lens!(Theme.accent))
}
// ANCHOR_END: table-presenting

// ANCHOR: table-cell-options
pub fn contact_table() -> Table<User> {
    Table::new((
        // Links to the user's page on another site, in a new tab.
        TextColumn::new(lens!(User.name))
            .url(|user: &User| Some(format!("https://people.example/{}", user.id)))
            .open_url_in_new_tab(),
        // A button copies the address; a long one shows its first 24 characters.
        TextColumn::new(lens!(User.email)).copyable().limit(24),
        // The role's `#[option(icon = …)]` in place of its label.
        IconColumn::new(lens!(User.role)),
    ))
    // A row whose user is inactive reads muted.
    .row_color(|user: &User| (!user.active).then_some(Color::Gray))
    .row_color_reading(Reads::of(lens!(User.active)))
}
// ANCHOR_END: table-cell-options

// ANCHOR: table-editable
pub fn triage_table() -> Table<Post> {
    Table::new((
        // Saves on Enter.
        TextInputColumn::new(lens!(Post.title)),
        // Each saves as soon as it changes.
        SelectColumn::new(lens!(Post.status)),
        ToggleColumn::new(lens!(Post.featured)),
    ))
}
// ANCHOR_END: table-editable

// ANCHOR: table-toggleable
pub fn user_table() -> Table<User> {
    Table::new((
        TextColumn::new(lens!(User.name)),
        // Shown until a reader hides it.
        TextColumn::new(lens!(User.email)).toggleable(),
        // Hidden until a reader shows it.
        DateColumn::new(lens!(User.created_at)).hidden_by_default(),
    ))
}
// ANCHOR_END: table-toggleable

// ANCHOR: table-derived-keys
#[derive(Debug, Clone, toasty::Model, tablo::ModelColumns)]
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
        |a: &Author| a.name.clone(),
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
        view! { cx => <span class="font-mono">(text)</span> }.boxed()
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
