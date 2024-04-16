-- :name create_variant :|| :1
-- :doc Creates a new variation for given feature
INSERT INTO variants(feature_id, value) VALUES($1, $2)
RETURNING variant_id

-- :name fetch_variant :<> :1
-- :doc Fetches a variant of given id
SELECT v.variant_id, feature_id, value, COALESCE(weight, 0) AS weight, acc
FROM variants v LEFT JOIN variants_weights vw ON v.variant_id = vw.variant_id AND vw.environment_id = $1
WHERE v.variant_id = $2

-- name fetch_variant_with_control_value :<> :1
-- doc Fetches a variant with control weight and value
-- SELECT v.variant_id, f.feature_id, f.value, 100-sum(weight) as weight, acc
-- FROM variants v JOIN variants_weights vw USING(variant_id)
-- JOIN features f USING (feature_id)
-- WHERE f.feature_id = $1

-- :name fetch_variants_for_feature :<> :*
-- :doc Fetches all variants for given feature
SELECT v.variant_id, feature_id, value, COALESCE(weight, 0) AS weight, acc
FROM variants v LEFT JOIN variants_weights vw ON v.variant_id = vw.variant_id AND vw.environment_id = $1
WHERE feature_id = $2
ORDER by v.variant_id

-- :name upsert_variant_weight :|| :1
-- :doc Inserts or updates a weight for feature variant in given environment
INSERT INTO variants_weights(environment_id, variant_id, weight) VALUES($1, $2, $3)
ON CONFLICT(variant_id, environment_id) DO UPDATE SET weight=$3
RETURNING weight

-- :name update_variant_value :<> :!
-- :doc Updates value of given feature variant
UPDATE variants SET value = $2
WHERE variant_id = $1

-- :name delete_variant :<> :!
-- :doc Removes variant of given id
DELETE FROM variants WHERE variant_id = $1

-- :name delete_variant_weights :<> :!
-- :doc Removes all variant weights
DELETE FROM variants_weights WHERE variant_id = $1

