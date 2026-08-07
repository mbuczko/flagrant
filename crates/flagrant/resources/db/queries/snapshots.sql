-- :name fetch_next_snapshot_version :<> :1
-- :doc Returns the next version number for a (feature, environment) pair - MAX(version)+1,
-- or 1 if no snapshot exists yet. Mirrors the MAX(N)+1 pattern already used for segment
-- group labels: versions are never reused, even across restores.
SELECT COALESCE(MAX(version), 0) + 1
FROM feature_snapshots
WHERE feature_id = $1 AND environment_id = $2

-- :name insert_snapshot :<> :1
-- :doc Inserts a new snapshot row and returns it in full.
INSERT INTO feature_snapshots(feature_id, environment_id, version, comment, state)
VALUES($1, $2, $3, $4, $5)
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
