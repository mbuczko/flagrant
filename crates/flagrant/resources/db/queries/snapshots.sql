-- :name insert_snapshot :<> :1
-- :doc Inserts a new snapshot row, computing its version as MAX(version)+1 for the
-- (feature, environment) pair in the same statement as the insert - avoids a
-- read-then-write race between two concurrent commits to the same feature+environment,
-- which could otherwise both compute the same "next" version and collide on the
-- UNIQUE(feature_id, environment_id, version) constraint.
INSERT INTO feature_snapshots(feature_id, environment_id, version, comment, state)
VALUES(
  $1, $2,
  (SELECT COALESCE(MAX(version), 0) + 1 FROM feature_snapshots WHERE feature_id = $1 AND environment_id = $2),
  $3, $4
)
RETURNING snapshot_id, feature_id, environment_id, version, comment, state, created_at

-- :name fetch_snapshots_for_feature :<> :*
-- :doc Returns every snapshot for a (feature, environment) pair, most recent first.
SELECT snapshot_id, feature_id, environment_id, version, comment, state, created_at
FROM feature_snapshots
WHERE feature_id = $1 AND environment_id = $2
ORDER BY version DESC

-- :name fetch_snapshot_by_version :<> :?
-- :doc Returns a single snapshot by (feature, environment, version).
SELECT snapshot_id, feature_id, environment_id, version, comment, state, created_at
FROM feature_snapshots
WHERE feature_id = $1 AND environment_id = $2 AND version = $3

-- :name update_snapshot_comment :<> :?
-- :doc Updates a snapshot's comment in place. Returns the updated row, or nothing if no
-- snapshot matches (feature, environment, version). Comment-only edit - state/version are
-- immutable once recorded.
UPDATE feature_snapshots SET comment = $4
WHERE feature_id = $1 AND environment_id = $2 AND version = $3
RETURNING snapshot_id, feature_id, environment_id, version, comment, state, created_at
