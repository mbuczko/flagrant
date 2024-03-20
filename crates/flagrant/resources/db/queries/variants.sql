-- :name create_variant :|| :1
-- :doc Creates a new variation for given feature
INSERT INTO variants(feature_id, value) VALUES($1, $2)
RETURNING variant_id

-- :name fetch_variant :<> :1
-- :doc Fetches a variant of given id
SELECT v.variant_id, feature_id, value, weight, acc
FROM variants v JOIN variants_weights vw USING(variant_id) 
WHERE environment_id = $1 AND v.variant_id = $2

-- name fetch_variant_with_control_value :<> :1
-- doc Fetches a variant with control weight and value
-- SELECT v.variant_id, f.feature_id, f.value, 100-sum(weight) as weight, acc
-- FROM variants v JOIN variants_weights vw USING(variant_id)
-- JOIN features f USING (feature_id)
-- WHERE f.feature_id = $1

-- :name fetch_variants_for_feature :<> :*
-- :doc Fetches all variants for given feature
SELECT v.variant_id, feature_id, value, weight, acc
FROM variants v JOIN variants_weights vw USING(variant_id) 
WHERE environment_id = $1 AND feature_id = $2

-- :name create_variant_weight :|| :1
-- :doc Creates a weight for feature variant in given environment
INSERT INTO variants_weights(environment_id, variant_id, weight) VALUES($1, $2, $3)
RETURNING weight

-- :name update_variant_weight :<> :!
-- :doc Updates weight of given feature variant
UPDATE variants_weights SET weight = $1
WHERE variant_id = $2 AND environment_id = $3

-- :name update_variant_value :<> :!
-- :doc Updates value of given feature variant
UPDATE variants SET value = $1
WHERE variant_id = $2

-- :name delete_variant :<> :!
-- :doc Removes variant of given id
DELETE FROM variants WHERE variant_id = $1
