use std::collections::HashMap;

use flagrant_types::{Environment, Variant};
use sqlx::{Connection, SqliteConnection};

use crate::models::{feature, variant};

/// Distributes a hit among the defined feature variants according to their associated weights.
/// `segment_id` scopes distribution to a segment's override weights (`None` = organic
/// default weights).
/// On every call:
///  - choose the variant with the largest `accumulator`
///  - subtract 100 from the `accumulator` of the chosen variant
///  - add `weight` to all variant accumulators, including the chosen one
///
/// Since every call only ever adds and subtracts, a pool holding `N` identities (`n_v` of
/// them on variant `v`) has `accumulator_v = weight_v * (N + 1) - 100 * n_v`: the variant's
/// deficit against its ideal share of the pool. Picking the largest accumulator therefore
/// always tops up the most under-represented variant - but only for as long as the
/// accumulators keep describing the pool's current population. An identity that leaves
/// the pool has to hand its draw back with [`release`], or its slot stays counted forever.
pub async fn distribute(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
    segment_id: Option<i32>,
) -> anyhow::Result<Variant> {
    pick(conn, environment, feature_id, segment_id, None).await
}

/// Like [`distribute`], for an identity that already holds `current_variant_id` and is only
/// changing pool (organic <-> a segment's, or between segments): it stays on that variant
/// as long as the new pool still has room for it, and only takes a fresh draw otherwise.
///
/// Room means a positive accumulator - the variant is still under its ideal share of the
/// pool (see [`distribute`]). Staying is accounted exactly like a draw that happened to land
/// on that variant, so the pool's accumulators stay consistent and its population still
/// ends up at its weights - but only as many identities change variant as the new weights
/// actually require, rather than nearly everybody being reshuffled by a fresh draw each
/// (e.g. a 10-point weight shift flipping ~60% of identities instead of ~10%).
///
/// The caller is expected to have already [`release`]d the identity's draw from the pool
/// it is leaving.
pub async fn redistribute(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
    segment_id: Option<i32>,
    current_variant_id: i32,
) -> anyhow::Result<Variant> {
    pick(
        conn,
        environment,
        feature_id,
        segment_id,
        Some(current_variant_id),
    )
    .await
}

async fn pick(
    conn: &mut SqliteConnection,
    environment: &Environment,
    feature_id: i32,
    segment_id: Option<i32>,
    keep: Option<i32>,
) -> anyhow::Result<Variant> {
    let mut tx = conn.begin().await?;
    let mut variants =
        variant::get_for_feature(&mut tx, environment, feature_id, segment_id).await?;

    let kept = keep.and_then(|id| {
        variants
            .iter()
            .position(|v| v.id == id && v.accumulator > 0)
    });
    let variant = match kept {
        Some(index) => variants.swap_remove(index),
        // There should always be at least one variant with a control value
        None => variants
            .into_iter()
            .max_by(|a, b| a.accumulator.cmp(&b.accumulator))
            .unwrap(),
    };

    variant::update_accumulator(
        &mut tx,
        environment,
        &variant,
        segment_id,
        variant.accumulator - 100,
    )
    .await?;
    feature::bump_up_accumulators(&mut tx, environment, feature_id, segment_id).await?;

    tx.commit().await?;
    Ok(variant)
}

/// Inverse of [`distribute`]: gives back `count` draws that landed on `variant_id` in the
/// pool scoped by `segment_id` (`None` = organic), for identities that left it - moved to
/// another pool, pinned, cleared or deleted.
///
/// Without it the accumulators keep counting an identity that is no longer there, and
/// whichever identities fill in later start a fresh cycle on top of it, so the pool's
/// population drifts away from its weights by however many identities came and went.
/// With it, the accumulators are as if the identity was never drawn, and the next draws
/// fill exactly the vacated deficit.
///
/// A no-op for a pool whose weights no longer exist (e.g. a removed segment override).
pub async fn release(
    conn: &mut SqliteConnection,
    environment_id: i32,
    feature_id: i32,
    segment_id: Option<i32>,
    variant_id: i32,
    count: i64,
) -> anyhow::Result<()> {
    if count > 0 {
        feature::release_accumulators(
            conn,
            environment_id,
            feature_id,
            segment_id,
            variant_id,
            count,
        )
        .await?;
    }
    Ok(())
}

/// How many of `total` identities each variant should hold to match its weight, given as
/// (variant id, weight) pairs - the exact shares rounded by the largest-remainder method, so
/// the counts always add up to `total` and no variant is off its exact share by a whole
/// identity. Ties go to the heavier variant, then the lower id, so the outcome is
/// deterministic.
///
/// Used when a pool that already holds identities gets new weights and has to be moved to
/// them in one go (see `identity::rebalance_segment_members`).
pub fn target_counts(weights: &[(i32, u8)], total: i64) -> HashMap<i32, i64> {
    let sum: i64 = weights.iter().map(|(_, w)| *w as i64).sum();
    let mut targets: HashMap<i32, i64> = weights.iter().map(|(id, _)| (*id, 0)).collect();

    if sum == 0 || total <= 0 {
        return targets;
    }

    // (variant id, weight, remainder of the exact share)
    let mut shares: Vec<(i32, u8, i64)> = weights
        .iter()
        .map(|(id, w)| {
            let exact = *w as i64 * total;
            targets.insert(*id, exact / sum);
            (*id, *w, exact % sum)
        })
        .collect();

    let leftover = total - targets.values().sum::<i64>();

    shares.sort_by(|a, b| b.2.cmp(&a.2).then(b.1.cmp(&a.1)).then(a.0.cmp(&b.0)));
    for (id, _, _) in shares.into_iter().take(leftover as usize) {
        *targets.get_mut(&id).unwrap() += 1;
    }
    targets
}
