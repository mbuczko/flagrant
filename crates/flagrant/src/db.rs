use rust_embed::RustEmbed;
use semver::Version;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow};
use sqlx::{Executor, Row, SqlitePool};
use std::{env, fmt, str};

#[derive(RustEmbed)]
#[folder = "resources/db/migrations/"]
struct Asset;

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Migration {
    file: String,
    version: String,
    description: String,
}

impl fmt::Display for Migration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{} => {} ({})",
            self.file, self.version, self.description
        )
    }
}

impl Migration {
    pub fn new(file: String, version: String, description: String) -> Self {
        Migration {
            file,
            version,
            description,
        }
    }
}

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

    Ok(pool)
}

pub async fn migrate(pool: &SqlitePool, app_semver: Version) -> anyhow::Result<Version> {
    let (last_script_version, last_app_version) = match version(pool).await {
        Ok((lsv, lav)) => (lsv, lav),
        Err(_) => (String::default(), Version::parse("0.0.0").unwrap()),
    };
    let v = match last_app_version {
        v if last_app_version < app_semver => {
            tracing::debug!("Upgrading DB schema from {v} to latest version: {app_semver}");
            upgrade(pool, last_script_version, app_semver).await?
        }
        v if last_app_version > app_semver => panic!(
            "Your app version {app_semver} is too old, minimal required version is: {v}"
        ),
        v => {
            tracing::debug!("DB schema already at newest version: {v}");
            v
        }
    };
    Ok(v)
}

async fn upgrade(
    pool: &SqlitePool,
    base_version: String,
    app_semver: Version,
) -> anyhow::Result<Version> {
    if let Some(migration) = build_migration(base_version, &app_semver) {
        match pool.execute(migration.as_str()).await {
            Ok(_) => {
                let (m, current_version) = version(pool).await?;
                tracing::debug!("Upgraded to latest migration {m}");
                return Ok(current_version);
            }
            Err(e) => panic!("Couldn't upgrade the schema. Bailing out: {e}")
        }
    }
    Ok(app_semver)
}

async fn version(pool: &SqlitePool) -> anyhow::Result<(String, Version)> {
    sqlx::query("SELECT version, app_semver FROM migrations ORDER BY version DESC LIMIT 1")
        .try_map(|row: SqliteRow| {
            Ok((
                row.try_get::<String, _>(0).unwrap(),
                Version::parse(row.try_get(1).unwrap()).unwrap(),
            ))
        })
        .fetch_one(pool)
        .await
        .map_err(Into::into)
}

fn build_migration(base_version: String, app_semver: &Version) -> Option<String> {
    let mut migrations: Vec<Migration> = Asset::iter()
        .filter_map(|res| {
            let v: Vec<&str> = res.split("__").collect();
            if v.len() == 2 {
                Some(Migration::new(
                    res.to_string(),
                    v[0].to_string(),
                    v[1].trim_end_matches(".sql").replace('_', " "),
                ))
            } else {
                None
            }
        })
        .collect();

    // sort migrations by versions first
    migrations.sort_by(|m1, m2| m1.version.cmp(&m2.version));

    // keep only those which haven't been applied yet
    migrations.retain(|m| base_version.is_empty() || m.version.gt(&base_version));

    // ...and compose final transaction
    let final_txn = migrations.iter().fold(String::default(), |mut txn, m| {
        let buf = Asset::get(m.file.as_ref()).unwrap();
        match str::from_utf8(buf.data.as_ref()) {
            Ok(s) => {
                txn.push_str(s);
                txn.push_str(
                    format!(
                        "\n\n\
                         INSERT INTO migrations(version, description, script, app_semver) \
                         VALUES ('{version}', '{description}', '{script}', '{semver}');\n\n",
                        version = m.version,
                        description = m.description,
                        script = m.file,
                        semver = app_semver
                    )
                    .as_str(),
                );
                txn
            }
            _ => panic!("Non UTF8 format of migration file!"),
        }
    });
    if final_txn.is_empty() {
        None
    } else {
        Some(format!("BEGIN TRANSACTION;\n\n{}\n\nCOMMIT;", final_txn))
    }
}
