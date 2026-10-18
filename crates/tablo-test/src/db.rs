//! The database a test runs on: in-memory SQLite, or the one `TABLO_TEST_DATABASE_URL` names.

use std::sync::{
    OnceLock,
    atomic::{AtomicU32, Ordering},
};

use toasty::Db;

/// The variable naming the database [`test_db`] connects to.
pub const TEST_DATABASE_URL: &str = "TABLO_TEST_DATABASE_URL";

/// A `Db` holding the tables of `models` and no rows.
///
/// The database is in-memory SQLite, fresh for each call, or the one [`TEST_DATABASE_URL`] names,
/// whose driver the test crate enables through Toasty's feature of the same name. Tests share that
/// database, so each call's tables carry a prefix of their own.
///
/// # Panics
///
/// When the database refuses the connection or the tables.
pub async fn test_db(models: toasty::schema::ModelSet) -> Db {
    let mut builder = Db::builder();
    builder.models(models);
    let url = match std::env::var(TEST_DATABASE_URL) {
        Ok(url) => {
            builder.table_name_prefix(&table_prefix());
            url
        }
        Err(_) => "sqlite::memory:".to_string(),
    };
    let db = builder.connect(&url).await.expect("connect");
    db.push_schema().await.expect("push_schema");
    db
}

/// A table-name prefix no other call of this run or of an earlier run uses. It stays short:
/// PostgreSQL truncates an identifier past 63 bytes, so a long prefix could make two index names
/// collide.
fn table_prefix() -> String {
    static RUN: OnceLock<u32> = OnceLock::new();
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let run = RUN.get_or_init(|| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        now.as_secs() as u32 ^ now.subsec_nanos()
    });
    let call = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("t{run:08x}{call}_")
}
