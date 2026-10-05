//! URLs into the request's panel, so app code never spells a prefix.
//!
//! Each helper answers for the panel serving the request: the one whose
//! prefix the request is under, or the only panel when the router mounts one.
//! A resource or page that panel does not register has no URL there, and the
//! helper returns `None`.
//!
//! ```text
//! let posts = tablo::url::resource::<PostResource>(cx);  // Some("/admin/posts")
//! let media = tablo::url::page::<MediaLibraryPage>(cx);  // Some("/admin/media")
//! let home = tablo::url::panel(cx);                      // Some("/admin")
//! ```

use std::any::TypeId;

use topcoat::context::Cx;

use super::state::current;
use crate::{Page, resource::Resource};

/// The request's panel prefix, e.g. `/admin`.
pub fn panel(cx: &Cx) -> Option<String> {
    current(cx).map(|panel| panel.prefix.clone())
}

/// `R`'s list URL in the request's panel, e.g. `/admin/posts`.
pub fn resource<R: Resource>(cx: &Cx) -> Option<String> {
    registered::<R>(cx)
}

/// `P`'s URL in the request's panel: `{prefix}/{slug}`, or the prefix itself
/// for the panel's [`home`](super::Panel::home) page.
pub fn page<P: Page>(cx: &Cx) -> Option<String> {
    registered::<P>(cx)
}

fn registered<T: 'static>(cx: &Cx) -> Option<String> {
    current(cx).and_then(|panel| panel.urls.get(&TypeId::of::<T>()).cloned())
}
