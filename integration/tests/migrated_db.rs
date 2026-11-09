//! A test database made by the app's migrations: what only a migration holds, here a check
//! constraint, refuses a write there, where a database pushed from the models takes it.

use tablo::testing::{TEST_DATABASE_URL, migrated_db};
use toasty::migration::{MigrationFile, MigrationSet};

use crate::common::test_db;

#[derive(Debug, Clone, toasty::Model)]
struct Owner {
    #[key]
    id: String,
    name: String,
}

/// The owners' table with a check the models cannot state: a name is never empty.
static MIGRATIONS: MigrationSet = MigrationSet::new(&[MigrationFile::new(
    0,
    "0000_owners.sql",
    "CREATE TABLE owners (id TEXT PRIMARY KEY, name TEXT NOT NULL CHECK (name <> ''))",
)]);

/// The same table in MySQL's words: a key is not `TEXT` there.
static MYSQL_MIGRATIONS: MigrationSet = MigrationSet::new(&[MigrationFile::new(
    0,
    "0000_owners.sql",
    "CREATE TABLE owners (id VARCHAR(64) PRIMARY KEY, name VARCHAR(191) NOT NULL \
     CHECK (name <> ''))",
)]);

fn migrations() -> &'static MigrationSet {
    match std::env::var(TEST_DATABASE_URL) {
        Ok(url) if url.starts_with("mysql:") || url.starts_with("mariadb:") => &MYSQL_MIGRATIONS,
        _ => &MIGRATIONS,
    }
}

async fn create(db: &toasty::Db, id: &str, name: &str) -> toasty::Result<Owner> {
    toasty::create!(Owner {
        id: id.to_string(),
        name: name.to_string(),
    })
    .exec(&mut db.clone())
    .await
}

#[tokio::test]
async fn a_migrated_database_holds_what_only_the_migrations_state() {
    let migrated = migrated_db(toasty::models!(Owner), migrations()).await;
    create(&migrated, "ada", "Ada")
        .await
        .expect("a named owner");
    assert!(
        create(&migrated, "nobody", "").await.is_err(),
        "the migration's check refuses an empty name"
    );

    let pushed = test_db(toasty::models!(Owner)).await;
    create(&pushed, "nobody", "")
        .await
        .expect("the models alone state no check");
}

/// Each call has tables of its own, on a shared database too: the second call creates its table
/// again, and reads none of the first call's rows.
#[tokio::test]
async fn each_migrated_database_is_its_own() {
    let first = migrated_db(toasty::models!(Owner), migrations()).await;
    create(&first, "ada", "Ada").await.expect("an owner");
    let second = migrated_db(toasty::models!(Owner), migrations()).await;
    let owners = Owner::all()
        .exec(&mut second.clone())
        .await
        .expect("list the owners");
    assert!(owners.is_empty(), "{owners:?}");
}
