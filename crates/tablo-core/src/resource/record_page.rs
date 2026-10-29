//! [`RecordPage`]: a page of the app's own about one record of a resource, served under the
//! record's URL, and the [`RecordPageDef`] it declares.

use std::{any::TypeId, future::Future, sync::Arc};

use topcoat::{
    Result,
    context::Cx,
    icon::IconData,
    view::{BoxView, View, ViewExt},
};

use super::{Resource, ResourceDef};
use crate::{
    DeclarationError, DeclarationErrorKind, Site, Text,
    naming::{derived_id, kebab_case, sentence_case, slug_case, type_stem},
};

/// A page about one record of the resource `R`, served at `{list}/{key}/{slug}` and linked from
/// the record's detail page. The panel loads the record and checks the request may open the page;
/// the page renders its body.
///
/// ```rust
/// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
/// # struct Order { #[key] #[auto] id: uuid::Uuid, number: String }
/// # struct OrderResource;
/// # impl tablo_core::Resource for OrderResource {
/// #     type Model = Order;
/// #     type Form = tablo_core::NoForm<Order>;
/// #     fn declare() -> tablo_core::ResourceDef<Self> {
/// #         tablo_core::ResourceDef::new().record_page::<OrderHistoryPage>()
/// #     }
/// # }
/// use tablo_core::RecordPage;
/// use topcoat::{
///     Result,
///     context::Cx,
///     view::{View, view},
/// };
///
/// struct OrderHistoryPage; // GET /admin/orders/{key}/order-history
///
/// impl RecordPage<OrderResource> for OrderHistoryPage {
///     async fn render(cx: &Cx, order: Order) -> Result<impl View> {
///         Ok(view! { cx => <p>"Order " (order.number)</p> })
///     }
/// }
/// ```
///
/// A request reaches [`render`](Self::render) only through the checks the detail page makes:
/// sign-in, the request's tenant, the resource's [`Resource::query`], and
/// [`Ability::View`](crate::Ability::View) of the record; then [`RecordPageDef::access`]. A
/// trashed record opens too, as it does on its detail page.
///
/// A form the page renders posts to an app route, which none of those checks cover: the route
/// loads the record through [`scoped_query`](crate::scoped_query) and asks the policy itself.
pub trait RecordPage<R: Resource>: Sized + Send + Sync + 'static {
    /// The page's slug, label, icon and access. Defaults to [`RecordPageDef::new`]: every
    /// default.
    fn declare() -> RecordPageDef<R::Model> {
        RecordPageDef::new()
    }

    /// Renders the page's body under the record's header, from the record as the resource's
    /// [`Resource::query`] loads it, with no relation included.
    ///
    /// The record arrives whole: a field a [`ResourceDef::field_access`] rule hides from the
    /// request is still on it, so a page that shows such a field asks
    /// [`can_view_field`](crate::can_view_field) first.
    fn render(cx: &Cx, record: R::Model) -> impl Future<Output = Result<impl View>> + Send;
}

/// Whether the request's user may open a record page for one record.
type AccessFn<M> = dyn Fn(&Cx, &M) -> bool + Send + Sync;

/// What a [`RecordPage`] declares: its slug, label, icon and access. Every setting has a default.
pub struct RecordPageDef<M> {
    slug: Option<String>,
    label: Option<Text>,
    icon: Option<IconData>,
    access: Arc<AccessFn<M>>,
}

impl<M> RecordPageDef<M> {
    /// A def with every default: the slug and label from the page's type name, and access for
    /// whoever may view the record.
    pub fn new() -> Self {
        Self::default()
    }

    /// The URL segment after the record's key: `"history"` serves the page at
    /// `{list}/{key}/history`.
    ///
    /// Defaults to the page type's name without a trailing `Page`, kebab-cased:
    /// `OrderHistoryPage` → `order-history`. Mounting refuses a slug that is not one URL path
    /// segment, `edit`, `delete`, `-`, or one another record page of the resource takes.
    #[must_use]
    pub fn slug(mut self, slug: impl Into<String>) -> Self {
        self.slug = Some(slug.into());
        self
    }

    /// The label of the page's link on the record's pages, and of its breadcrumb.
    ///
    /// Defaults to the message `page-{name}` of the panel's catalogs, else the page type's name
    /// without a trailing `Page`, in sentence case: `OrderHistoryPage` reads `page-order-history`,
    /// else `Order history`.
    #[must_use]
    pub fn label(mut self, label: impl Into<Text>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The icon of the page's link.
    #[must_use]
    pub fn icon(mut self, icon: IconData) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Whether the request's user may open the page for `record`, asked once the policy allows
    /// [`Ability::View`](crate::Ability::View) of it. The panel answers 403 and links no page it
    /// answers `false` for. Defaults to `true`.
    ///
    /// Read no relation of `record`: each page asking loads the record with relations of its own,
    /// the record page with none.
    #[must_use]
    pub fn access(mut self, access: impl Fn(&Cx, &M) -> bool + Send + Sync + 'static) -> Self {
        self.access = Arc::new(access);
        self
    }
}

impl<M> Default for RecordPageDef<M> {
    fn default() -> Self {
        Self {
            slug: None,
            label: None,
            icon: None,
            access: Arc::new(|_, _| true),
        }
    }
}

impl<M> std::fmt::Debug for RecordPageDef<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordPageDef")
            .field("slug", &self.slug)
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

impl<R: Resource> ResourceDef<R> {
    /// Adds the [`RecordPage`] `P` after the ones already declared, served at
    /// `{list}/{key}/{slug}` for each record and linked from the record's detail page.
    #[must_use]
    pub fn record_page<P: RecordPage<R>>(mut self) -> Self {
        self.record_pages.push(RecordPageEntry::new::<P>());
        self
    }
}

/// Renders a record page's body.
type RenderFn<M> = for<'a> fn(&'a Cx, M) -> crate::BoxFuture<'a, Result<BoxView<'a>>>;

/// A [`RecordPageDef`] with its defaults filled in for the page `P`, its type erased.
pub(crate) struct RecordPageEntry<R: Resource> {
    pub(crate) page: TypeId,
    pub(crate) name: &'static str,
    pub(crate) slug: String,
    pub(crate) label: Text,
    pub(crate) icon: Option<IconData>,
    access: Arc<AccessFn<R::Model>>,
    pub(crate) render: RenderFn<R::Model>,
}

impl<R: Resource> RecordPageEntry<R> {
    fn new<P: RecordPage<R>>() -> Self {
        let def = P::declare();
        let stem = type_stem::<P>("Page");
        let label = def.label.unwrap_or_else(|| {
            let noun = sentence_case(stem);
            Text::derived(derived_id("page", &slug_case(&noun)), noun)
        });
        Self {
            page: TypeId::of::<P>(),
            name: std::any::type_name::<P>(),
            slug: def.slug.unwrap_or_else(|| kebab_case(stem)),
            label,
            icon: def.icon,
            access: def.access,
            render: render::<R, P>,
        }
    }

    /// Whether the request's user may open the page for `record`, once `View` allows it.
    pub(crate) fn admits(&self, cx: &Cx, record: &R::Model) -> bool {
        (self.access)(cx, record)
    }
}

impl<R: Resource> std::fmt::Debug for RecordPageEntry<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordPageEntry")
            .field("name", &self.name)
            .field("slug", &self.slug)
            .finish_non_exhaustive()
    }
}

fn render<R: Resource, P: RecordPage<R>>(
    cx: &Cx,
    record: R::Model,
) -> crate::BoxFuture<'_, Result<BoxView<'_>>> {
    Box::pin(async move { Ok(P::render(cx, record).await?.boxed()) })
}

/// The segments a record's own routes take after its key, which no record page may.
const RESERVED_SEGMENTS: &[&str] = &["edit", "delete", "-"];

/// Refuses a record page of `R` whose slug `validate` refuses as a URL path segment, is one of
/// the record's own routes, or is another record page's. Answers whether it refused none.
pub(crate) fn check<R: Resource>(
    pages: &[RecordPageEntry<R>],
    validate: impl Fn(&str) -> Option<DeclarationErrorKind>,
    errors: &mut Vec<DeclarationError>,
) -> bool {
    let before = errors.len();
    for (index, page) in pages.iter().enumerate() {
        let slug = page.slug.clone();
        let kind = if let Some(kind) = validate(&slug) {
            kind
        } else if RESERVED_SEGMENTS.contains(&slug.as_str()) {
            DeclarationErrorKind::ReservedSlug { slug: slug.clone() }
        } else if pages[..index].iter().any(|earlier| earlier.slug == slug) {
            DeclarationErrorKind::DuplicateSlug { slug: slug.clone() }
        } else {
            continue;
        };
        errors.push(DeclarationError::of::<R>(Site::RecordPage(slug), kind));
    }
    errors.len() == before
}
