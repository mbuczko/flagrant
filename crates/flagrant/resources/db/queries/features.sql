-- :name create_feature :|| :1
-- :doc Creates a new feature with name, on/off status and value type
INSERT INTO features(project_id, name, description, is_enabled) VALUES($1, $2, $3, $4)
RETURNING feature_id, project_id, name, description, is_enabled, archived_at

-- :name fetch_feature_by_id :|| :1
-- :doc Returns a feature of given id (without corresponding variants)
SELECT f.feature_id, project_id, name, description, is_enabled, archived_at, GROUP_CONCAT(ft.tag, ',') AS tags
FROM features f
LEFT JOIN feature_tags ft ON ft.feature_id = f.feature_id
WHERE f.feature_id = $1
GROUP BY f.feature_id

-- :name fetch_feature_by_name :|| :1
-- :doc Returns a feature with provided name
SELECT f.feature_id, project_id, name, description, is_enabled, archived_at, GROUP_CONCAT(ft.tag, ',') AS tags
FROM features f
LEFT JOIN feature_tags ft ON ft.feature_id = f.feature_id
WHERE project_id = $1 AND name = $2
GROUP BY f.feature_id

-- :name fetch_features_for_environment :|| :*
-- :doc Returns all features for given environment, each with all its variants.
WITH feature_tag_groups AS (
  SELECT feature_id, GROUP_CONCAT(tag, ',') AS tags
  FROM feature_tags AS ft
  WHERE 1=1
--~{ tags_included
  AND ft.tag IN (SELECT value FROM json_each($6))
--~}
--~{ tags_excluded
  AND ft.tag NOT IN (SELECT value FROM json_each($7))
--~}
  GROUP BY feature_id
)
SELECT f.feature_id, f.project_id, f.name, f.description, f.is_enabled, f.archived_at,
       v.variant_id, v.environment_id, v.value,
       COALESCE(vw.weight, 0) AS weight, vw.accumulator,
       ftg.tags
FROM features f
LEFT JOIN variants v ON v.feature_id = f.feature_id AND COALESCE(v.environment_id, $2) = $2
LEFT JOIN variant_weights vw ON vw.variant_id = v.variant_id AND vw.environment_id = $2
LEFT JOIN feature_tag_groups ftg ON ftg.feature_id = f.feature_id
WHERE f.project_id = $1
--~{ is_archived
AND ($3 = (f.archived_at IS NOT NULL))
--~}
--~{ is_enabled
AND f.is_enabled = $4
--~}
--~{ pattern
AND f.name LIKE($5)
--~}
ORDER BY f.is_enabled DESC, f.archived_at ASC, f.name, weight DESC

-- :name update_feature :<> :!
-- :doc Updates feature with new values of name and is_enabled flag
UPDATE features
SET name = $2, is_enabled = $3
WHERE feature_id = $1

-- :name update_feature_description :<> :!
-- :doc Updates feature description
UPDATE features SET description = $2 WHERE feature_id = $1

-- :name archive_feature :<> :!
-- :doc Updates feature archivisation timestamp. If NULL then feature is not archived.
UPDATE features SET archived_at = $2 WHERE feature_id = $1

-- :name update_feature_variants_accumulators :<> :!
-- :doc Bumps accumulators for feature variants, scoped to a segment (NULL = organic)
UPDATE variant_weights
SET accumulator = accumulator + weight
WHERE environment_id = $1 AND variant_id IN (select variant_id from variants where feature_id = $2)
  AND segment_id IS $3

-- :name delete_feature :<> :!
-- :doc Removes a feature. Note that feature value and variants need to be removed before.
DELETE FROM features WHERE feature_id = $1

-- :name delete_variants_for_feature :<> :!
-- :doc Removes all variants for a feature (all environments).
DELETE FROM variants WHERE feature_id = $1

-- :name delete_variant_weights_for_feature :<> :!
-- :doc Removes all variant_weights rows (organic and segment-scoped) for all variants of a feature (across all environments).
DELETE FROM variant_weights WHERE variant_id IN (SELECT variant_id FROM variants WHERE feature_id = $1)

-- :name delete_identity_variants_for_feature :<> :!
-- :doc Removes all identity_variants rows for a feature (across all environments).
DELETE FROM identity_variants WHERE feature_id = $1

-- :name delete_tags_for_feature :<> :!
-- :doc Removes a feature tags.
DELETE FROM feature_tags WHERE feature_id = $1

-- :name insert_tag_for_feature :<> :!
-- :doc Adds a single tag for a feature, unless it is already present.
INSERT INTO feature_tags(feature_id, tag)
SELECT $1, $2
WHERE NOT EXISTS (
  SELECT 1 FROM feature_tags WHERE feature_id = $1 AND tag = $2
)

-- :name delete_tag_for_feature :<> :!
-- :doc Removes a single tag from a feature.
DELETE FROM feature_tags WHERE feature_id = $1 AND tag = $2
