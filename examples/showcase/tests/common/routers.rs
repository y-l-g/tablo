//! The routers the integration tests mount: the showcase panel without
//! filesystem assets, with and without the demo uploader.

use showcase::app::build_router;
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

/// Build the showcase router with uploads stored as the application stores them, in a fresh
/// directory under the system's temporary directory rather than the source tree.
///
/// The media library's page writes through this configuration — the panel's
/// `serve_dir` mount and the store are two ends of one directory.
pub fn router_with_app_uploads(db: Db) -> Router {
    let dir = std::env::temp_dir().join(format!("showcase-uploads-{}", uuid::Uuid::new_v4()));
    build_router(db, None, Some(dir))
}
