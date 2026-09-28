use sqlx::SqlitePool;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::env;
use std::fs::OpenOptions;

static MIGRATOR: Migrator = sqlx::migrate!();

/// Opens (creating if missing, without truncating) the file `FLAGRANT_DB` points at, to
/// surface a clear, actionable error - wrong directory, read-only filesystem, permission
/// denied - before handing the path to sqlx, whose own "unable to open database file"
/// error carries none of that detail.
fn ensure_writable(path: &str) -> std::io::Result<()> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(drop)
}

pub async fn init_pool() -> anyhow::Result<SqlitePool> {
    let db_path = env::var("FLAGRANT_DB").map_err(|_| {
        tracing::error!("FLAGRANT_DB environment variable is not set");
        anyhow::anyhow!("FLAGRANT_DB environment variable is not set")
    })?;

    if let Err(err) = ensure_writable(&db_path) {
        tracing::error!(path = %db_path, error = %err, "FLAGRANT_DB location is not writable");
        anyhow::bail!("FLAGRANT_DB location ({db_path}) is not writable: {err}");
    }

    let options = SqliteConnectOptions::new()
        .filename(db_path)
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
