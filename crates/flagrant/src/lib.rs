use anyhow::Result;
use semver::Version;
use sqlx::{Pool, Sqlite};

pub mod db;
pub mod distributor;
pub mod errors;
pub mod models;

pub use flagrant_macros::test;

pub async fn init() -> Result<Pool<Sqlite>> {
    let pool = db::init_pool()
        .await
        .expect("Could not connect to database");

    db::migrate(&pool, Version::parse(env!("CARGO_PKG_VERSION")).unwrap()).await?;
    Ok(pool)
}
