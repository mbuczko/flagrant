-- :name create_feature :|| :1
-- :doc Creates a new feature with name and on/off flag
INSERT INTO features(project_id, name, is_enabled) VALUES($1, $2, $3)
RETURNING feature_id, project_id, name, is_enabled

-- :name create_feature_value :<> :!
-- :doc Creates feature value within given environment
INSERT INTO features_values(environment_id, feature_id, value, value_type) VALUES($1, $2, $3, $4)

-- :name fetch_feature :|| :1
-- :doc Returns a feature with value corresponding to given environment_id
SELECT f.feature_id, f.project_id, f.name, fv.value, fv.value_type, is_enabled
FROM features f LEFT OUTER JOIN features_values fv USING(feature_id)
WHERE COALESCE(fv.environment_id, $1) = $1 AND f.feature_id = $2

-- :name fetch_feature_by_name :|| :1
-- :doc Returns a feature with provided name
SELECT f.feature_id, f.project_id, f.name, fv.value, fv.value_type, is_enabled
FROM features f LEFT OUTER JOIN features_values fv USING(feature_id)
WHERE COALESCE(fv.environment_id, $1) = $1 AND f.project_id = $2 AND f.name = $3

-- :name fetch_features_for_environment :|| :*
-- :doc Fetches all features for given environment
SELECT f.feature_id, f.project_id, f.name, fv.value, fv.value_type, is_enabled
FROM features f LEFT OUTER JOIN features_values fv USING(feature_id)
WHERE COALESCE(fv.environment_id, $1) = $1 AND f.project_id = $2

-- :name update_feature :<> :!
-- :doc Updates feature with new values of name and is_enabled flag
UPDATE features
SET name = $2, is_enabled = $3
WHERE feature_id = $1

-- :name update_feature_value :<> :!
-- :doc Updates feature value for given environment
UPDATE features_values
SET value = $3, value_type = $4
WHERE environment_id = $1 AND feature_id = $2
