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

    /// Distributes hit among defined variants in respect to associated weights.
    /// On every call:
    ///  - choose the variation with the largest `accum`
    ///  - subtract 100 from the `accum` for the chosen variation
    ///  - add `weight` to `accum` for all variations, including the chosen one
    pub async fn distribute(&self, pool: &Pool<Sqlite>, environment: &Environment) -> anyhow::Result<Variant> {
        let mut variants = variant::list(pool, environment, &self.feature).await?;
        let max_accum = variants
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.accumulator.cmp(&b.accumulator));

        // there should be always at least one variation with a control value
        let (idx, _) = max_accum.unwrap();
        let var = variants.swap_remove(idx);

        let mut tx = pool.begin().await?;
        variant::update_accumulator(&mut tx, environment, &var, var.accumulator - 100).await?;
        feature::bump_up_accumulators(&mut tx, environment, &self.feature, var.weight).await?;

        tx.commit().await?;
        Ok(var)
    }

}
