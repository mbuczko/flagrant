use std::collections::{HashMap, HashSet};

use flagrant_types::{
    Environment, Feature, Project, Segment, Snapshot, SnapshotIdentityOverride,
    SnapshotSegmentGroup, SnapshotSegmentOverride, SnapshotState, SnapshotVariant, TagList,
    Variant,
    payload::{FeaturePatch, TagPatchOp, VariantPatchOp},
};
use hugsqlx::{HugSqlx, params};
use sqlx::{Connection, SqliteConnection};

use super::{feature, identity, rollout, rule, segment, variant};
use crate::errors::FlagrantError;

#[derive(HugSqlx)]
#[queries = "resources/db/queries/snapshots.sql"]
struct SQLSnapshots {}

/// Captures the full materialized state of `feature` within `environment` right after a
/// commit, and inserts it as the next version for this (feature, environment) pair. Callers
/// are expected to run this inside the same transaction as the commit it's recording.
pub async fn capture(
    conn: &mut SqliteConnection,
    project: &Project,
    environment: &Environment,
    feature: &Feature,
    comment: Option<String>,
) -> anyhow::Result<Snapshot> {
    let state = build_state(conn, project, environment, feature).await?;
    let state_json = serde_json::to_string(&state)?;

    let snapshot = SQLSnapshots::insert_snapshot::<_, Snapshot>(
        conn,
        params![feature.id, environment.id, comment, state_json],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not insert snapshot", e))?;

    Ok(snapshot)
}

/// Lists every snapshot for a (feature, environment) pair, most recent first.
pub async fn list(
    conn: &mut SqliteConnection,
    feature_id: i32,
    environment_id: i32,
) -> anyhow::Result<Vec<Snapshot>> {
    SQLSnapshots::fetch_snapshots_for_feature::<_, Snapshot>(
        conn,
        params![feature_id, environment_id],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not list snapshots", e).into())
}

/// Fetches a single snapshot by its version within a (feature, environment) pair.
pub async fn get_by_version(
    conn: &mut SqliteConnection,
    feature_id: i32,
    environment_id: i32,
    version: i32,
) -> anyhow::Result<Snapshot> {
    let row = SQLSnapshots::fetch_snapshot_by_version::<_, Snapshot>(
        conn,
        params![feature_id, environment_id, version],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not fetch snapshot", e))?;

    row.ok_or(FlagrantError::NotFound("Snapshot not found").into())
}

/// Updates a snapshot's comment in place - the only field of a recorded snapshot that's
/// ever mutated after the fact; state and version are fixed at capture time. Not part of
/// the staged feature/identity/segment patch cycle - like `IDENTITY delete` or `SEGMENT
/// delete`, this applies immediately, since a past snapshot has no "current entity in
/// context" to stage a patch against.
pub async fn set_comment(
    conn: &mut SqliteConnection,
    feature_id: i32,
    environment_id: i32,
    version: i32,
    comment: Option<String>,
) -> anyhow::Result<Snapshot> {
    let row = SQLSnapshots::update_snapshot_comment::<_, Snapshot>(
        conn,
        params![feature_id, environment_id, version, comment],
    )
    .await
    .map_err(|e| FlagrantError::QueryFailed("Could not update snapshot comment", e))?;

    row.ok_or(FlagrantError::NotFound("Snapshot not found").into())
}

async fn build_state(
    conn: &mut SqliteConnection,
    project: &Project,
    environment: &Environment,
    feature: &Feature,
) -> anyhow::Result<SnapshotState> {
    let variants = feature
        .variants
        .iter()
        .map(|v| SnapshotVariant {
            id: v.id,
            value: v.value.clone(),
            weight: v.weight,
            is_control: v.is_control(),
        })
        .collect();

    let overrides = segment::list_overrides_for_feature(conn, environment.id, feature.id).await?;
    let mut segment_overrides = Vec::with_capacity(overrides.len());

    for (segment_id, _name, weights) in overrides {
        let seg = segment::get_by_id(conn, project, segment_id).await?;
        let groups = seg
            .groups
            .into_iter()
            .map(|g| SnapshotSegmentGroup {
                label: g.label,
                connector: g.connector,
                description: g.description,
                rules: g.rules,
            })
            .collect();

        segment_overrides.push(SnapshotSegmentOverride {
            segment_id: seg.id,
            segment_name: seg.name,
            segment_description: seg.description,
            groups,
            weights,
        });
    }

    let identity_overrides = identity::list_pinned_overrides(conn, environment.id, feature.id)
        .await?
        .into_iter()
        .map(
            |(identity_id, identity_value, variant_id)| SnapshotIdentityOverride {
                identity_id,
                identity_value,
                variant_id,
            },
        )
        .collect();

    // Captured together, not just the step - a step index only means something relative
    // to the exact schedule it was recorded against.
    let rollout_step = if feature.rollout.is_some() {
        rollout::current_step(conn, feature.id, environment.id).await?
    } else {
        None
    };

    Ok(SnapshotState {
        name: feature.name.clone(),
        description: feature.description.clone(),
        is_enabled: feature.is_enabled,
        is_srv: feature.is_srv,
        is_archived: feature.is_archived,
        rollout_config: feature.rollout.clone(),
        tags: feature.tags.0.iter().map(|t| t.name.clone()).collect(),
        variants,
        segment_overrides,
        identity_overrides,
        rollout_step,
    })
}

/// Diffs a live [`TagList`] against a snapshot's flat tag list, producing the
/// add/remove ops needed to reconcile the former into the latter.
fn diff_tags(current: &TagList, target: &[String]) -> Vec<TagPatchOp> {
    let current_names: HashSet<&str> = current.0.iter().map(|t| t.name.as_str()).collect();
    let target_names: HashSet<&str> = target.iter().map(|s| s.as_str()).collect();

    target_names
        .iter()
        .filter(|name| !current_names.contains(*name))
        .map(|name| TagPatchOp::Add((*name).to_string()))
        .chain(
            current_names
                .iter()
                .filter(|name| !target_names.contains(*name))
                .map(|name| TagPatchOp::Remove((*name).to_string())),
        )
        .collect()
}

/// Result of reconciling a snapshot's variants against the feature's current live variants.
struct VariantReconciliation {
    /// Ops to feed into `feature::patch`'s `variants` field.
    patch_ops: Vec<VariantPatchOp>,
    /// Snapshot variant id -> resolved current variant id. Entries for variants that had
    /// to be recreated (`Add` op) are filled in by the caller after the patch is applied
    /// and the new ids are known.
    id_map: HashMap<i32, i32>,
}

/// Reconciles a snapshot's variants against the feature's current live variants, matching
/// first by id, then (for a snapshot variant whose id no longer exists live) by value -
/// since non-control variant values are unique per feature, a live variant already carrying
/// the "same" value under a different id (e.g. deleted then manually re-added) is reused
/// instead of emitting a duplicate `Add` that would collide with that uniqueness constraint.
/// Only genuinely new values fall through to `Add`.
fn reconcile_variants(current: &[Variant], snapshot: &[SnapshotVariant]) -> VariantReconciliation {
    let mut ops = Vec::new();
    let mut id_map = HashMap::new();
    let mut matched: HashSet<i32> = HashSet::new();

    let current_control_id = current.iter().find(|v| v.is_control()).map(|v| v.id);

    for sv in snapshot {
        if sv.is_control {
            if let Some(cid) = current_control_id {
                id_map.insert(sv.id, cid);
                matched.insert(cid);

                let cur_value = current.iter().find(|v| v.id == cid).map(|v| &v.value);

                if cur_value != Some(&sv.value) {
                    ops.push(VariantPatchOp::SetValue {
                        id: cid,
                        value: sv.value.clone(),
                    });
                }
            }
            continue;
        }

        if let Some(cur) = current
            .iter()
            .find(|v| !v.is_control() && v.id == sv.id && !matched.contains(&v.id))
        {
            id_map.insert(sv.id, cur.id);
            matched.insert(cur.id);

            if cur.value != sv.value {
                ops.push(VariantPatchOp::SetValue {
                    id: cur.id,
                    value: sv.value.clone(),
                });
            }
            if cur.weight != sv.weight {
                ops.push(VariantPatchOp::SetWeight {
                    id: cur.id,
                    weight: sv.weight,
                });
            }
            continue;
        }

        if let Some(cur) = current
            .iter()
            .find(|v| !v.is_control() && v.value == sv.value && !matched.contains(&v.id))
        {
            id_map.insert(sv.id, cur.id);
            matched.insert(cur.id);

            if cur.weight != sv.weight {
                ops.push(VariantPatchOp::SetWeight {
                    id: cur.id,
                    weight: sv.weight,
                });
            }
            continue;
        }

        // Neither id nor value matched anything live - recreate. Its resolved id is
        // filled into `id_map` by `restore` once the patch has been applied.
        ops.push(VariantPatchOp::Add {
            value: sv.value.clone(),
            weight: sv.weight,
        });
    }

    for cur in current.iter().filter(|v| !v.is_control()) {
        if !matched.contains(&cur.id) {
            ops.push(VariantPatchOp::Delete { id: cur.id });
        }
    }

    VariantReconciliation {
        patch_ops: ops,
        id_map,
    }
}

/// Recreates a segment from a snapshot's stored definition, used when the segment a
/// snapshot's override pointed at no longer exists. Suffixes the name if it collides with
/// an unrelated segment that has since taken it, rather than failing the whole restore.
async fn recreate_segment(
    conn: &mut SqliteConnection,
    project: &Project,
    seg_override: &SnapshotSegmentOverride,
) -> anyhow::Result<Segment> {
    let name = if segment::get_by_name(conn, project, &seg_override.segment_name)
        .await
        .is_ok()
    {
        format!("{} (restored)", seg_override.segment_name)
    } else {
        seg_override.segment_name.clone()
    };

    let created = segment::create(
        conn,
        project,
        name,
        seg_override.segment_description.clone(),
    )
    .await?;

    for g in &seg_override.groups {
        let group =
            segment::add_group(conn, &created, g.description.clone(), g.connector.clone()).await?;

        for r in &g.rules {
            rule::add(
                conn,
                created.id,
                group.id,
                r.subject.clone(),
                r.comparator.clone(),
                r.value.clone(),
            )
            .await?;
        }
    }

    segment::get_by_id(conn, project, created.id).await
}

/// Restores `feature` (within `environment`) to the state captured by snapshot `version`.
///
/// Restoring is itself a commit: it ends by capturing a brand-new snapshot whose state
/// matches the target version, so version numbers stay strictly increasing and nothing is
/// ever renumbered or lost. Segment override weights are restored (recreating the segment
/// from its stored definition if it was deleted); pinned identity overrides are restored by
/// remapping their variant through the same reconciliation used for variants/segment
/// weights; organic (non-pinned) assignments are always cleared and left to lazy
/// redistribution, never restored.
pub async fn restore(
    conn: &mut SqliteConnection,
    project: &Project,
    environment: &Environment,
    feature: &Feature,
    version: i32,
    comment: Option<String>,
) -> anyhow::Result<Snapshot> {
    let target = get_by_version(conn, feature.id, environment.id, version).await?;
    let state = target.parsed_state()?;

    let mut tx = conn.begin().await?;

    let recon = reconcile_variants(&feature.variants, &state.variants);
    let mut id_map = recon.id_map;

    let patch = FeaturePatch {
        name: (feature.name != state.name).then(|| state.name.clone()),
        is_enabled: (feature.is_enabled != state.is_enabled).then_some(state.is_enabled),
        is_archived: (feature.is_archived != state.is_archived).then_some(state.is_archived),
        is_srv: (feature.is_srv != state.is_srv).then_some(state.is_srv),
        description: (feature.description != state.description).then(|| state.description.clone()),
        tags: diff_tags(&feature.tags, &state.tags),
        variants: recon.patch_ops,
        rollout: None,
        delete: false,
    };

    let updated = feature::patch(&mut tx, environment, feature, patch)
        .await?
        .ok_or_else(|| {
            anyhow::Error::from(FlagrantError::UnexpectedFailure(
                "Restore unexpectedly deleted the feature",
                anyhow::anyhow!("feature::patch returned None"),
            ))
        })?;

    // Fill in id_map entries for variants that were just recreated (`Add` ops) - now that
    // they have real ids, match by value (unique per feature among non-control variants).
    let mut used_ids: HashSet<i32> = id_map.values().copied().collect();
    let pending_adds: Vec<&SnapshotVariant> = state
        .variants
        .iter()
        .filter(|sv| !sv.is_control && !id_map.contains_key(&sv.id))
        .collect();
    for sv in pending_adds {
        if let Some(cur) = updated
            .variants
            .iter()
            .find(|v| !v.is_control() && v.value == sv.value && !used_ids.contains(&v.id))
        {
            id_map.insert(sv.id, cur.id);
            used_ids.insert(cur.id);
        }
    }

    // Segments currently overriding this feature that aren't reapplied below (because the
    // target snapshot predates them) must be cleared - otherwise an override added *after*
    // the restored version would incorrectly survive the restore.
    let mut stale_segment_ids: HashSet<i32> =
        segment::list_overrides_for_feature(&mut tx, environment.id, updated.id)
            .await?
            .into_iter()
            .map(|(segment_id, ..)| segment_id)
            .collect();

    for seg_override in &state.segment_overrides {
        let segment = match segment::get_by_id(&mut tx, project, seg_override.segment_id).await {
            Ok(s) => s,
            Err(_) => recreate_segment(&mut tx, project, seg_override).await?,
        };
        stale_segment_ids.remove(&segment.id);

        variant::delete_segment_weights_for_feature(
            &mut tx,
            segment.id,
            updated.id,
            environment.id,
        )
        .await?;

        for w in &seg_override.weights {
            if let Some(&current_variant_id) = id_map.get(&w.variant_id) {
                variant::set_segment_weight(
                    &mut tx,
                    environment,
                    segment.id,
                    current_variant_id,
                    w.weight,
                )
                .await?;
            }
        }
        variant::balance_segment_control_weight(&mut tx, environment, segment.id, updated.id)
            .await?;
        identity::mark_feature_dirty(&mut tx, environment.id, updated.id).await?;
    }

    for segment_id in stale_segment_ids {
        variant::delete_segment_weights_for_feature(
            &mut tx,
            segment_id,
            updated.id,
            environment.id,
        )
        .await?;
        identity::mark_feature_dirty(&mut tx, environment.id, updated.id).await?;
    }

    identity::clear_variants_for_feature_environment(&mut tx, environment.id, updated.id).await?;
    for pin in &state.identity_overrides {
        let Some(&current_variant_id) = id_map.get(&pin.variant_id) else {
            continue;
        };
        let resolved = match identity::get_by_id(&mut tx, environment, pin.identity_id).await {
            Ok(i) => Some(i),
            Err(_) => identity::get_by_value(&mut tx, environment, pin.identity_value.clone())
                .await
                .ok(),
        };
        if let Some(identity) = resolved {
            identity::override_variant(
                &mut tx,
                environment,
                &identity,
                updated.id,
                current_variant_id,
            )
            .await?;
        }
    }

    // Restore the rollout config and step together, atomically - a step index only means
    // something relative to the exact schedule it was recorded against, which may have
    // since been replaced by a different one (a rule change resets progression to step 0
    // going forward, but that offers no protection for an *older* snapshot predating the
    // change). Bypasses `RolloutPatchOp::Set`'s normal "always reset to step 0" behavior
    // via `feature::restore_rollout_config`, since we're setting a specific historical
    // step right after, not activating a fresh rollout.
    match &state.rollout_config {
        Some(cfg) => {
            feature::restore_rollout_config(&mut tx, updated.id, Some(cfg)).await?;
            match state.rollout_step {
                Some(step) => rollout::set_step(&mut tx, environment.id, updated.id, step).await?,
                None => rollout::activate(&mut tx, environment.id, updated.id).await?,
            }
        }
        None if updated.rollout.is_some() => {
            feature::restore_rollout_config(&mut tx, updated.id, None).await?;
            rollout::deactivate(&mut tx, updated.id).await?;
        }
        None => {}
    }

    let default_comment = format!("restored from v{version}");
    let snapshot = capture(
        &mut tx,
        project,
        environment,
        &updated,
        Some(comment.unwrap_or(default_comment)),
    )
    .await?;

    tx.commit().await?;
    Ok(snapshot)
}
