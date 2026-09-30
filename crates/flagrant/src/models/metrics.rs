use flagrant_types::VariantValue;
use hugsqlx::{HugSqlx, params};
use sqlx::SqliteConnection;

use crate::errors::FlagrantError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/metrics.sql"]
struct SQLMetrics {}

/// Number of identities in a single environment.
#[derive(Debug, sqlx::FromRow)]
pub struct IdentityTotal {
    pub project: String,
    pub environment: String,
    pub identities: i64,
}

/// Number of identities currently assigned to a single variant of a feature, within one
/// environment.
#[derive(Debug, sqlx::FromRow)]
pub struct VariantIdentityCount {
    pub project: String,
    pub environment: String,
    pub feature: String,
    pub variant_id: i32,
    pub variant: VariantValue,
    pub identities: i64,
}

/// Number of identities attributed to a segment that currently have a given variant of a
/// feature, within one environment.
#[derive(Debug, sqlx::FromRow)]
pub struct SegmentVariantIdentityCount {
    pub project: String,
    pub environment: String,
    pub feature: String,
    pub segment: String,
    pub variant_id: i32,
    pub variant: VariantValue,
    pub identities: i64,
}

/// Number of identity assignments of a feature flagged for lazy re-evaluation after a
/// segment change (`segment_dirty`), within one environment.
#[derive(Debug, sqlx::FromRow)]
pub struct DirtyIdentityCount {
    pub project: String,
    pub environment: String,
    pub feature: String,
    pub identities: i64,
}

/// Number of identities carrying a trait (whatever its value), within one environment.
#[derive(Debug, sqlx::FromRow)]
pub struct TraitIdentityCount {
    pub project: String,
    pub environment: String,
    pub trait_name: String,
    pub identities: i64,
}

/// Returns identity totals for every environment of every project.
pub async fn identity_totals(conn: &mut SqliteConnection) -> anyhow::Result<Vec<IdentityTotal>> {
    SQLMetrics::fetch_identity_totals::<_, IdentityTotal>(conn, params!())
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not count identities", e).into())
}

/// Returns, for every variant of every feature in every environment, the number of
/// identities assigned to it. Variants nobody is assigned to are reported with zero.
///
/// Counts follow an identity's *effective* variant, i.e. a pending weight-migration
/// (`migrated_id`) wins over the variant it was originally attached to.
pub async fn variant_identity_counts(
    conn: &mut SqliteConnection,
) -> anyhow::Result<Vec<VariantIdentityCount>> {
    SQLMetrics::fetch_variant_identity_counts::<_, VariantIdentityCount>(conn, params!())
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not count identities per variant", e).into())
}

/// Returns, for every trait in every environment of its project, the number of identities
/// carrying it. Traits nobody carries are reported with zero.
pub async fn trait_identity_counts(
    conn: &mut SqliteConnection,
) -> anyhow::Result<Vec<TraitIdentityCount>> {
    SQLMetrics::fetch_trait_identity_counts::<_, TraitIdentityCount>(conn, params!())
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not count identities per trait", e).into())
}

/// Returns, for every segment overriding (or still holding identities of) a feature, the
/// number of attributed identities per variant of that feature. Variants nobody attributed
/// to the segment is assigned to are reported with zero.
pub async fn segment_variant_identity_counts(
    conn: &mut SqliteConnection,
) -> anyhow::Result<Vec<SegmentVariantIdentityCount>> {
    SQLMetrics::fetch_segment_variant_identity_counts::<_, SegmentVariantIdentityCount>(
        conn,
        params!(),
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not count identities per segment", e).into())
}

/// Returns, for every feature in every environment, how many identity assignments are
/// still waiting to be re-evaluated against changed segments (zero when none).
pub async fn dirty_identity_counts(
    conn: &mut SqliteConnection,
) -> anyhow::Result<Vec<DirtyIdentityCount>> {
    SQLMetrics::fetch_dirty_identity_counts::<_, DirtyIdentityCount>(conn, params!())
        .await
        .map_err(|e| FlagrantError::QueryFailed("Could not count dirty identities", e).into())
}
