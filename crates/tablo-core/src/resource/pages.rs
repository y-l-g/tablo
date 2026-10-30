//! [`ResourcePage`]: the record pages a resource serves beside its list, and
//! [`ResourceDef::without_page`], which turns one off.

use super::{Resource, ResourceDef};

/// A record page a resource serves beside its list, unless
/// [`without_page`](ResourceDef::without_page) turns it off.
///
/// A resource serves its list, and these pages by default:
///
/// - the create page and the edit page, when it has a record form: a resource whose `Form` is
///   [`NoForm`](crate::NoForm) serves neither;
/// - the view page, which shows its [`detail`](ResourceDef::detail).
///
/// The policy decides who may open a page, per request, but never whether it exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ResourcePage {
    /// The create page, for a resource whose records arrive another way: a seed, an action.
    /// Without it, mounting skips the check that a create fills every non-nullable column, the
    /// list and the relations offer no create button, and an
    /// [`importable`](ResourceDef::importable) resource refuses to mount.
    Create,
    /// The view page. Turned off, its route answers 404 and no row links to it.
    View,
}

impl<R: Resource> ResourceDef<R> {
    /// Turns `page` off.
    ///
    /// ```rust
    /// # #[derive(Debug, Clone, toasty::Model, tablo_core::ModelColumns)]
    /// # struct Post { #[key] #[auto] id: uuid::Uuid, title: String }
    /// # struct PostResource;
    /// # impl tablo_core::Resource for PostResource {
    /// #     type Model = Post;
    /// #     type Form = tablo_core::NoForm<Post>;
    /// # }
    /// use tablo_core::{ResourceDef, ResourcePage};
    ///
    /// ResourceDef::<PostResource>::new().without_page(ResourcePage::View);
    /// ```
    #[must_use]
    pub fn without_page(mut self, page: ResourcePage) -> Self {
        match page {
            ResourcePage::Create => self.pages.create = false,
            ResourcePage::View => self.pages.view = false,
        }
        self
    }
}

/// The record pages a resource serves: on a [`ResourceDef`], the ones it has not turned off; on
/// a mounted resource, the ones it serves.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pages {
    pub(crate) create: bool,
    pub(crate) view: bool,
    pub(crate) edit: bool,
}

impl Default for Pages {
    fn default() -> Self {
        Self {
            create: true,
            view: true,
            edit: true,
        }
    }
}

impl Pages {
    /// The pages a resource that turned off none of `declared` serves, with a record form when
    /// `has_form`.
    pub(crate) fn served(declared: Self, has_form: bool) -> Self {
        Self {
            create: has_form && declared.create,
            view: declared.view,
            edit: has_form && declared.edit,
        }
    }
}
