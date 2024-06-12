use flagrant_types::{Environment, Feature, Variant};
use sqlx::{Pool, Sqlite};

use crate::models::{feature, variant};

pub struct Distributor {
    feature: Feature,
}

impl Distributor {
    pub fn new(feature: Feature) -> Self {
        Self { feature }
    }

    /// Distributes hit among defined featured variants in respect to associated weights.
    /// On every call:
    ///  - choose the variation with the largest `accum`
    ///  - subtract 100 from the `accum` for the chosen variation
    ///  - add `weight` to `accum` for all variations, including the chosen one
    pub async fn distribute(
        &self,
        pool: &Pool<Sqlite>,
        environment: &Environment,
    ) -> anyhow::Result<Variant> {
        let variants = variant::list(pool, environment, &self.feature).await?;

        // there should be always at least one variation with a control value
        let variant = variants
            .into_iter()
            .max_by(|a, b| a.accumulator.cmp(&b.accumulator))
            .unwrap();

        let mut tx = pool.begin().await?;

        variant::update_accumulator(&mut tx, environment, &variant, variant.accumulator - 100).await?;
        feature::bump_up_accumulators(&mut tx, environment, &self.feature).await?;

        tx.commit().await?;
        Ok(variant)
    }
}
