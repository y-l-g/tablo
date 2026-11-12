//! [`mounted_resources`]: the resources the request's panel mounts, for what reads across them.

use topcoat::context::Cx;

use super::{relations::Child, state::current};
use crate::Text;

/// A resource the request's panel mounts, as [`mounted_resources`] lists it: what a feature that
/// reads across resources, such as an activity feed, needs to name it and to mask it.
#[derive(Clone, Copy)]
pub struct MountedResource<'a>(&'a Child);

impl<'a> MountedResource<'a> {
    /// Its slug, as its URLs spell it: `posts`. An area may mount another resource at the same
    /// slug.
    pub fn slug(&self) -> &'a str {
        &self.0.slug
    }

    /// Its label: `Post`.
    pub fn label(&self) -> &'a Text {
        &self.0.label
    }

    /// Its plural label: `Posts`.
    pub fn plural_label(&self) -> &'a Text {
        &self.0.plural_label
    }

    /// Whether `cx` may open its list, as [`can_list`](crate::can_list) answers.
    pub fn can_list(&self, cx: &Cx) -> bool {
        (self.0.listable)(cx)
    }

    /// The fields [`field_access`](crate::ResourceDef::field_access) hides from `cx` on it;
    /// `None` when `cx` answers as another panel, which a caller reads as every field hidden.
    pub fn hidden_fields(&self, cx: &Cx) -> Option<Vec<String>> {
        (self.0.hidden)(cx)
    }
}

impl std::fmt::Debug for MountedResource<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MountedResource")
            .field(&self.0.slug)
            .finish()
    }
}

/// Each resource the request's panel mounts, in no particular order; none outside every panel.
///
/// ```rust
/// # use tablo_core::mounted_resources;
/// # use topcoat::context::Cx;
/// /// The slugs whose list `cx` may open.
/// fn listable(cx: &Cx) -> Vec<&str> {
///     mounted_resources(cx)
///         .into_iter()
///         .filter(|resource| resource.can_list(cx))
///         .map(|resource| resource.slug())
///         .collect()
/// }
/// ```
pub fn mounted_resources(cx: &Cx) -> Vec<MountedResource<'_>> {
    current(cx).map_or_else(Vec::new, |panel| {
        panel.children.values().map(MountedResource).collect()
    })
}
