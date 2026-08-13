-- :name fetch_rollout_state :<> :?
-- :doc Returns live progression state for a (feature, environment) pair, if a rollout
-- has ever been activated there.
SELECT feature_id, environment_id, current_step, last_change_at, created_at
FROM feature_rollout_state
WHERE feature_id = $1 AND environment_id = $2

-- :name activate_rollout :<> :!
-- :doc Seeds/reactivates progression state for a (feature, environment) pair, resetting
-- it back to step 0.
INSERT INTO feature_rollout_state(feature_id, environment_id, current_step, last_change_at)
VALUES ($1, $2, 0, CURRENT_TIMESTAMP)
ON CONFLICT(feature_id, environment_id) DO UPDATE SET current_step = 0, last_change_at = CURRENT_TIMESTAMP

-- :name set_rollout_state :<> :!
-- :doc Force-sets progression state to an exact step, used by snapshot::restore - which
-- restores to a specific historical step rather than resetting to 0 like activate_rollout.
INSERT INTO feature_rollout_state(feature_id, environment_id, current_step, last_change_at)
VALUES ($1, $2, $3, CURRENT_TIMESTAMP)
ON CONFLICT(feature_id, environment_id) DO UPDATE SET current_step = $3, last_change_at = CURRENT_TIMESTAMP

-- :name delete_rollout_state_for_feature :<> :!
-- :doc Removes progression state for a feature across every environment - used when a
-- rollout is disabled, since the rule list it depends on is gone.
DELETE FROM feature_rollout_state WHERE feature_id = $1

-- :name count_distributed_identities :<> :1
-- :doc Counts organically-distributed (non-pinned) identities for a (feature, environment)
-- pair - the input to the minimum-sample-size gate.
SELECT COUNT(*) FROM identity_variants
WHERE feature_id = $1 AND environment_id = $2 AND pinned_at IS NULL

-- :name advance_rollout_state :<> :?
-- :doc Compare-and-swap: advances current_step/last_change_at only if current_step still
-- matches the caller's previously-read value. Returns the updated row, or nothing if
-- another request already won the race and advanced it first.
UPDATE feature_rollout_state
SET current_step = $3, last_change_at = $4
WHERE feature_id = $1 AND environment_id = $2 AND current_step = $5
RETURNING feature_id, environment_id, current_step, last_change_at, created_at
