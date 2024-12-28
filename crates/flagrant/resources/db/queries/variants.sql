-- :name create_variant :|| :1
-- :doc Creates a new variant for given feature
INSERT INTO variants(feature_id, value)
VALUES($1, $2)
RETURNING variant_id

-- :name upsert_control_variant :|| :1
-- :doc Creates or updates control variant for given feature
INSERT INTO variants(environment_id, feature_id, value)
VALUES($1, $2, $3)
ON CONFLICT(environment_id, feature_id) DO UPDATE SET value = excluded.value
RETURNING variant_id

-- :name upsert_control_variant_weight :<> :1
-- :doc Inserts or updates a weight for feature control variant in given environment
WITH calc AS
  (select coalesce(100 - sum(weight), 100) AS remaining_weight, $1 as environment_id
    from variant_weights w
    join variants v using(variant_id)
    where feature_id = $2
      -- exclude control value from sum...
      and v.environment_id is null
      -- ...and sum all the other variant weights within given environment
      and w.environment_id = $1)
INSERT INTO variant_weights(environment_id, variant_id, accumulator, weight)
SELECT
  $1,
  v.variant_id,
  calc.remaining_weight,
  calc.remaining_weight
FROM variants v
JOIN calc USING(environment_id)
LEFT JOIN variant_weights vw USING(variant_id)
WHERE v.environment_id = $1 AND v.feature_id = $2
ON CONFLICT(environment_id, variant_id)
DO UPDATE SET accumulator = excluded.accumulator, weight = excluded.weight
RETURNING variant_id, weight

-- :name upsert_variant_weight :<> :!
-- :doc Inserts or updates a weight (and accumulator) for feature variant in given environment
INSERT INTO variant_weights(environment_id, variant_id, weight, accumulator)
VALUES($1, $2, $3, $3)
ON CONFLICT(environment_id, variant_id) DO UPDATE SET weight = excluded.weight, accumulator = excluded.accumulator

-- :name update_variant_value :|| :1
-- :doc Updates value of given feature variant
UPDATE variants SET value = $2
WHERE environment_id IS NULL AND variant_id = $1
RETURNING feature_id

-- :name update_variant_accumulator :<> :!
-- :doc Updates accumulator of given feature variant
UPDATE variant_weights SET accumulator = $3
WHERE environment_id = $1 AND variant_id = $2

-- :name fetch_variant :<> :1
-- :doc Fetches a variant of given id. Control variant value is automatically calculated.
SELECT v.variant_id, v.environment_id, feature_id, value, COALESCE(weight, 0) AS weight, accumulator
FROM variants v
LEFT JOIN variant_weights vw ON v.variant_id = vw.variant_id AND vw.environment_id = $1
WHERE v.variant_id = $2

-- :name fetch_variants_for_feature :<> :*
-- :doc Fetches all variants for given feature
SELECT v.variant_id, v.environment_id, v.value, COALESCE(vw.weight, 0) AS weight, vw.accumulator
FROM variants v
LEFT JOIN variant_weights vw ON vw.variant_id = v.variant_id AND vw.environment_id = $1
WHERE feature_id = $2 AND COALESCE(v.environment_id, $1) = $1
ORDER BY weight DESC

-- :name fetch_variants_for_identity :<> :*
-- :doc Fetches feature variants for given identity. Variants attached to identity by distributor are denoted by non-NULL identity_id field.
SELECT f.feature_id, v.variant_id, f.name, v.value, iv.migrated_id, max(iv.identity_id) AS identity_id
FROM features f
JOIN variants v USING(feature_id)
LEFT JOIN identities i ON i.identity = lower($2)
LEFT JOIN identity_variants iv ON iv.variant_id = v.variant_id AND iv.identity_id = i.identity_id
WHERE f.is_enabled = true AND COALESCE(v.environment_id, $1) = $1
GROUP BY f.feature_id
ORDER BY identity_id DESC

-- :name fetch_count_of_feature_variants :<> :1
-- :doc Fetches a number of variants that belong to same feature that given variant_id belongs to
SELECT v1.feature_id, count(v2.variant_id) AS count
FROM variants v1 JOIN variants v2 USING(feature_id)
WHERE COALESCE(v2.environment_id, $1) = $1 AND v1.variant_id = $2

-- :name delete_variant :<> :!
-- :doc Removes variant of given id
DELETE FROM variants WHERE variant_id = $1

-- :name delete_variant_weights :<> :!
-- :doc Removes all variant weights
DELETE FROM variant_weights WHERE variant_id = $1
