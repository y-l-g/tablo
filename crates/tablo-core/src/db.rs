//! The `Db` glue: how Tablo code reaches the pooled Toasty database.

use toasty::Db;
use topcoat::context::{Cx, app_context};

/// Returns the pooled [`Db`] registered on the app context.
///
/// Never runs statements on another handle while a transaction holds its connection; drains or drops streaming list bodies before the next query.
#[inline]
pub fn db(cx: &Cx) -> Db {
    app_context::<Db>(cx).clone()
}

#[cfg(test)]
mod tests;
