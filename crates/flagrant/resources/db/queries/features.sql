-- :name create_feature :<> :1
-- :doc Creates a new feature with name, value and on/off flag
INSERT INTO features(project_id, name, value, is_enabled) VALUES($1, $2, $3, $4)
RETURNING feature_id, project_id, name, value, is_enabled

-- :name fetch_feature :<> :1
-- :doc Fetches a feature of given id
SELECT feature_id, project_id, name, value, is_enabled
FROM features
WHERE feature_id = $1

-- :name fetch_feature_by_name :<> :1
-- :doc Fetches a feature of given id
SELECT feature_id, project_id, name, value, is_enabled
FROM features
WHERE project_id = $1 AND name = $2

-- :name fetch_features_for_project :<> :*
-- :doc Fetches all features for given project
SELECT feature_id, project_id, name, value, is_enabled
FROM features
WHERE project_id = $1
