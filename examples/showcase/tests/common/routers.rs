//! The routers the integration tests mount: the showcase panel without
//! filesystem assets, with and without the demo uploader.

use showcase::app::{build_router, upload_dir};
use toasty::Db;
use topcoat::router::Router;

/// Build the showcase router without filesystem assets for markup tests.
///
/// This is deliberately separate from `router`: the application path fails
/// loudly when its generated bundle is missing, while tests can exercise the
/// server-rendered markup without pretending an asset bundle exists.
///
/// It installs **no uploader** either, which pins the framework's default for
/// a file field with no store. A test that needs the demo store uses
/// `router_with_app_uploads`.
pub fn router_for_tests(db: Db) -> Router {
    build_router(db, None, None)
}

/// Build the showcase router with uploads at the directory the application
/// itself uses.
///
/// The media library's page writes through this configuration — the panel's
/// `serve_dir` mount and the store are two ends of one directory.
pub fn router_with_app_uploads(db: Db) -> Router {
    build_router(db, None, Some(upload_dir()))
}
