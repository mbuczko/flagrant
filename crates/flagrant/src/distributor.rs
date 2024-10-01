use flagrant_types::{Environment, Variant};
use sqlx::{Connection, SqliteConnection};

use crate::models::{feature, variant};

/// Distributes hit among defined featured variants in respect to associated weights.
/// On every call:
///  - choose the variation with the largest `accum`
///  - subtract 100 from the `accum` for the chosen variation
///  - add `weight` to `accum` for all variations, including the chosen one
pub async fn distribute(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: u16,
) -> anyhow::Result<Variant> {
    let mut tx = conn.begin().await?;
    let variants = variant::get_all(&mut tx, environment, feature_id).await?;

    // there should be always at least one variation with a control value
    let variant = variants
        .into_iter()
        .max_by(|a, b| a.accumulator.cmp(&b.accumulator))
        .unwrap();

    variant::update_accumulator(&mut tx, environment, &variant, variant.accumulator - 100).await?;
    feature::bump_up_accumulators(&mut tx, environment, feature_id).await?;

    tx.commit().await?;
    Ok(variant)
}
