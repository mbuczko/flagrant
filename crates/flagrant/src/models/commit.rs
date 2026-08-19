use std::collections::HashSet;

use flagrant_types::{
    Environment, Project,
    payload::{CommitPayload, CommitResult, SegmentPatchOp},
};
use sqlx::{Connection, SqliteConnection};

use super::{environment, feature, identity, segment, snapshot};

/// Applies a single `COMMIT` as one atomic, server-side operation: whichever of the
/// feature/identity/segment parts are present in `payload` are applied together in one
/// transaction, then every `(feature_id, environment_id)` pair affected by the combined
/// change gets exactly one new snapshot - never zero (a segment-only commit silently
/// changing a feature's effective state with no history recorded), never more than one
/// (uncoordinated writes from what the user experienced as a single commit).
///
/// `environment` is the environment the CLI session is currently in - identity overrides
/// and segment weight-override ops are always scoped to it; a segment's *structural*
/// change (rules/groups/name) can additionally affect other environments, resolved below.
pub async fn apply(
    conn: &mut SqliteConnection,
    project: &Project,
    environment: &Environment,
    payload: CommitPayload,
) -> anyhow::Result<CommitResult> {
    let mut tx = conn.begin().await?;
    let mut affected: HashSet<(i32, i32)> = HashSet::new();

    let updated_feature = if let Some(part) = payload.feature {
        let current = feature::get_by_id(&mut tx, environment, part.id).await?;
        let result = feature::patch(&mut tx, environment, &current, part.patch).await?;

        if result.is_some() {
            affected.insert((part.id, environment.id));
        }
        result
    } else {
        None
    };

    let updated_identity = if let Some(part) = payload.identity {
        let current = identity::get_by_value(&mut tx, environment, part.value.clone()).await?;

        // Overrides/unset_overrides name features by value, not id - resolve to ids up
        // front so we can record affected pairs regardless of what identity::patch does
        // internally. Skipped entirely when the patch stages a delete: identity::patch
        // ignores overrides/unset_overrides in that case (the identity - and its
        // overrides - are just gone), so resolving them here would record pairs that
        // were never actually applied.
        if !part.patch.delete {
            for ovr in &part.patch.overrides {
                if let Ok(feat) =
                    feature::get_by_name(&mut tx, environment, &ovr.feature_name).await
                {
                    affected.insert((feat.id, environment.id));
                }
            }
            for feature_name in &part.patch.unset_overrides {
                if let Ok(feat) = feature::get_by_name(&mut tx, environment, feature_name).await {
                    affected.insert((feat.id, environment.id));
                }
            }
        }
        identity::patch(&mut tx, environment, current, part.patch).await?
    } else {
        None
    };

    let updated_segment = if let Some(part) = payload.segment {
        let current = segment::get_by_id(&mut tx, project, part.id).await?;

        let weight_only = !part.patch.ops.is_empty()
            && part.patch.ops.iter().all(|op| {
                matches!(
                    op,
                    SegmentPatchOp::SetFeatureOverride { .. }
                        | SegmentPatchOp::UnsetFeatureOverride { .. }
                )
            });

        if weight_only {
            // Weight-only ops are always scoped to this commit's own environment - see
            // segment::patch.
            for op in &part.patch.ops {
                match op {
                    SegmentPatchOp::SetFeatureOverride { feature_id, .. }
                    | SegmentPatchOp::UnsetFeatureOverride { feature_id } => {
                        affected.insert((*feature_id, environment.id));
                    }
                    _ => {}
                }
            }
        } else if !part.patch.ops.is_empty() {
            // Structural change (rename/describe/group/rule) or a delete - the segment's
            // embedded definition (or its very existence) is now stale in every feature it
            // currently overrides, in every environment, since segment structure itself
            // isn't environment-scoped. Gathered *before* applying the patch so a delete
            // still resolves to the segment's overrides as they were right before removal.
            for env in environment::get_by_project(&mut tx, project).await? {
                for ov in segment::list_overridden_features(&mut tx, env.id, part.id).await? {
                    affected.insert((ov.feature_id, env.id));
                }
            }
        }
        segment::patch(&mut tx, project, environment, current, part.patch).await?
    } else {
        None
    };

    let mut snapshots = Vec::with_capacity(affected.len());

    for (feature_id, environment_id) in affected {
        let env = environment::get_by_id(&mut tx, environment_id).await?;
        let feat = feature::get_by_id(&mut tx, &env, feature_id).await?;
        let snap =
            snapshot::capture(&mut tx, project, &env, &feat, payload.comment.clone()).await?;

        snapshots.push(snap);
    }

    tx.commit().await?;

    Ok(CommitResult {
        feature: updated_feature,
        identity: updated_identity,
        segment: updated_segment,
        snapshots,
    })
}
