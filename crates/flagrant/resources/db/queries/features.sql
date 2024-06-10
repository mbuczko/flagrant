-- :name create_feature :|| :1
-- :doc Creates a new feature with name, on/off status and value type
INSERT INTO features(project_id, name, is_enabled) VALUES($1, $2, $3)
RETURNING feature_id, project_id, name, is_enabled

-- :name fetch_feature :|| :1
-- :doc Returns a feature of given id (without corresponding variants)
SELECT feature_id, project_id, name, is_enabled
FROM features
WHERE feature_id = $1

-- :name fetch_feature_by_name :|| :1
-- :doc Returns a feature with provided name
SELECT feature_id, project_id, name, is_enabled
FROM features
WHERE project_id = $1 AND name = $2

-- :name fetch_features_by_pattern :|| :*
-- :doc Returns a list of features with names matching given pattern. Each feature is returned along with its control value only.
SELECT f.feature_id, f.project_id, f.name, f.is_enabled, v.variant_id, v.value
FROM features f
LEFT OUTER JOIN variants v ON v.feature_id = f.feature_id AND v.environment_id = $2
WHERE f.project_id = $1 AND f.name LIKE $3
ORDER by length(f.name)

-- :name fetch_features_for_environment :|| :*
-- :doc Returns all features for given environment. Each feature is returned along with its control value only.
SELECT f.feature_id, f.project_id, f.name, f.is_enabled, v.variant_id, v.value
FROM features f
LEFT OUTER JOIN variants v ON v.feature_id = f.feature_id AND v.environment_id = $2
WHERE f.project_id = $1

-- :name update_feature :<> :!
-- :doc Updates feature with new values of name and is_enabled flag
UPDATE features
SET name = $2, is_enabled = $3
WHERE feature_id = $1

-- :name update_feature_variants_accumulators :<> :!
-- :doc Updates feature variants accumulators by given value
UPDATE variants_weights
SET accumulator = accumulator + weight
WHERE environment_id = $1 AND variant_id IN (select variant_id from variants where feature_id = $2)

-- :name delete_feature :<> :!
-- :doc Removes a feature. Note that feature value and variants need to be removed before.
DELETE FROM features WHERE feature_id = $1

-- :name delete_variants_for_feature :<> :!
-- :doc Removes a feature value.
DELETE FROM variants WHERE feature_id = $1
