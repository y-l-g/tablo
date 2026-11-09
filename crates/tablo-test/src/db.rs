//! The database a test runs on: in-memory SQLite, or the one `TABLO_TEST_DATABASE_URL` names,
//! its tables pushed from the models or made by the app's migrations.

use std::sync::{
    OnceLock,
    atomic::{AtomicU32, Ordering},
};

use toasty::{Db, migration::MigrationSet, schema::ModelSet};

/// The variable naming the database [`test_db`] connects to.
pub const TEST_DATABASE_URL: &str = "TABLO_TEST_DATABASE_URL";

/// A `Db` holding the tables of `models` and no rows.
///
/// The database is in-memory SQLite, fresh for each call, or the one [`TEST_DATABASE_URL`] names,
/// whose driver the test crate enables through Toasty's feature of the same name. Tests share that
/// database, so each call's tables carry a prefix of their own. An app whose migrations add what
/// the models cannot state, such as a foreign key, tests on [`migrated_db`] instead.
///
/// # Panics
///
/// When the database refuses the connection or the tables.
pub async fn test_db(models: ModelSet) -> Db {
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

/// A `Db` over `models` whose tables `migrations` made, holding no rows: what the app's
/// migrations add beside the models, such as a foreign key or a check constraint written by
/// hand, holds in the test too, where [`test_db`] pushes the models' schema alone.
///
/// ```rust,ignore
/// static MIGRATIONS: MigrationSet = toasty::embed_migrations!();
///
/// let db = migrated_db(app::models(), &MIGRATIONS).await;
/// ```
///
/// Migrations are SQL for one database, so the test runs where the app does: the database
/// [`TEST_DATABASE_URL`] names, else in-memory SQLite, fresh for each call. Migration SQL names
/// its tables in full, so on a shared database each call gets a namespace of its own instead of a
/// table prefix: a PostgreSQL schema its connections search, or a MySQL database. Neither is
/// dropped afterwards, as [`test_db`]'s tables are not.
///
/// # Panics
///
/// When the database refuses the connection, the namespace or a migration.
pub async fn migrated_db(models: ModelSet, migrations: &MigrationSet) -> Db {
    let url = match std::env::var(TEST_DATABASE_URL) {
        Ok(url) => isolated(models.clone(), &url).await,
        Err(_) => "sqlite::memory:".to_string(),
    };
    let mut builder = Db::builder();
    builder.models(models);
    let db = builder.connect(&url).await.expect("connect");
    migrations.apply(&db).await.expect("apply the migrations");
    db
}

/// The URL of a namespace of its own on the shared database `url` names, made on the way: a
/// PostgreSQL schema the connection searches first, or a MySQL database. Another database, such
/// as an in-memory one, is fresh for each connection already.
async fn isolated(models: ModelSet, url: &str) -> String {
    let (scheme, _) = url.split_once(':').unwrap_or((url, ""));
    let create = match scheme {
        "postgres" | "postgresql" => "SCHEMA",
        "mysql" | "mariadb" => "DATABASE",
        _ => return url.to_string(),
    };
    let name = table_prefix().trim_end_matches('_').to_string();
    let mut builder = Db::builder();
    builder.models(models);
    let mut admin = builder.connect(url).await.expect("connect");
    toasty::sql::statement(format!("CREATE {create} {name}"))
        .exec(&mut admin)
        .await
        .expect("create the test's namespace");
    if create == "SCHEMA" {
        let separator = if url.contains('?') { '&' } else { '?' };
        format!("{url}{separator}options=-c%20search_path%3D{name}")
    } else {
        with_database(url, &name)
    }
}

/// `url` naming the database `name` in place of its own.
fn with_database(url: &str, name: &str) -> String {
    let (base, query) = match url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (url, None),
    };
    let authority_start = base.find("://").map_or(0, |at| at + 3);
    let path_start = base[authority_start..]
        .find('/')
        .map_or(base.len(), |at| authority_start + at);
    let mut out = format!("{}/{name}", &base[..path_start]);
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    out
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

#[cfg(test)]
mod tests {
    use super::with_database;

    #[test]
    fn a_mysql_url_names_the_tests_database() {
        assert_eq!(
            with_database("mysql://root:pw@localhost:3306/tablo", "t1"),
            "mysql://root:pw@localhost:3306/t1"
        );
        assert_eq!(
            with_database("mysql://localhost?ssl-mode=disabled", "t1"),
            "mysql://localhost/t1?ssl-mode=disabled"
        );
    }
}
