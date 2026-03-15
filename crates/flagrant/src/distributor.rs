use flagrant_types::{Environment, Variant};
use sqlx::{Connection, SqliteConnection};

use crate::models::{feature, variant};

/// Distributes a hit among the defined feature variants according to their associated weights.
/// On every call:
///  - choose the variant with the largest `accumulator`
///  - subtract 100 from the `accumulator` of the chosen variant
///  - add `weight` to all variant accumulators, including the chosen one
pub async fn distribute(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
) -> anyhow::Result<Variant> {
    let mut tx = conn.begin().await?;
    let variants = variant::get_all(&mut tx, environment, feature_id).await?;

    // There should always be at least one variant with a control value
    let variant = variants
        .into_iter()
        .max_by(|a, b| a.accumulator.cmp(&b.accumulator))
        .unwrap();

    variant::update_accumulator(&mut tx, environment, &variant, variant.accumulator - 100).await?;
    feature::bump_up_accumulators(&mut tx, environment, feature_id).await?;

    tx.commit().await?;
    Ok(variant)
}
