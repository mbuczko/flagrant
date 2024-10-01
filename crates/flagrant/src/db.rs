use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::env;

static MIGRATOR: Migrator = sqlx::migrate!();

pub async fn init_pool() -> anyhow::Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(env::var("DB_NAME").expect("No DB_NAME provided"))
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .min_connections(1)
        .max_connections(5)
        .test_before_acquire(true)
        .connect_with(options)
        .await?;

    MIGRATOR.run(&pool).await?;
    Ok(pool)
}
