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
-- :doc Inserts or updates the organic (non-segment) weight for feature control variant in given environment
WITH calc AS
  (select coalesce(100 - sum(weight), 100) AS remaining_weight, $1 as environment_id
    from variant_weights w
    join variants v using(variant_id)
    where feature_id = $2
      -- exclude control value from sum...
      and v.environment_id is null
      -- ...and sum all the other organic variant weights within given environment
      and w.environment_id = $1
      and w.segment_id is null)
INSERT INTO variant_weights(environment_id, variant_id, segment_id, accumulator, weight)
SELECT
  $1,
  v.variant_id,
  NULL,
  calc.remaining_weight,
  calc.remaining_weight
FROM variants v
JOIN calc USING(environment_id)
WHERE v.environment_id = $1 AND v.feature_id = $2
ON CONFLICT(environment_id, variant_id) WHERE segment_id IS NULL
DO UPDATE SET accumulator = excluded.accumulator, weight = excluded.weight
RETURNING variant_id, weight

-- :name upsert_variant_weight :<> :!
-- :doc Inserts or updates the organic (non-segment) weight (and accumulator) for feature variant in given environment
INSERT INTO variant_weights(environment_id, variant_id, segment_id, weight, accumulator)
VALUES($1, $2, NULL, $3, $3)
ON CONFLICT(environment_id, variant_id) WHERE segment_id IS NULL
DO UPDATE SET weight = excluded.weight, accumulator = excluded.accumulator

-- :name upsert_segment_variant_weight :<> :!
-- :doc Inserts or updates a segment-scoped weight (and accumulator) for feature variant in given environment
INSERT INTO variant_weights(environment_id, variant_id, segment_id, weight, accumulator)
VALUES($1, $2, $3, $4, $4)
ON CONFLICT(variant_id, environment_id, segment_id) WHERE segment_id IS NOT NULL
DO UPDATE SET weight = excluded.weight, accumulator = excluded.accumulator

-- :name upsert_segment_control_variant_weight :<> :1
-- :doc Inserts or updates the remainder weight for feature control variant within a segment
WITH calc AS
  (select coalesce(100 - sum(weight), 100) AS remaining_weight, $1 as environment_id
    from variant_weights w
    join variants v using(variant_id)
    where feature_id = $2
      -- exclude control value from sum...
      and v.environment_id is null
      -- ...and sum all the other variant weights within given environment and segment
      and w.environment_id = $1
      and w.segment_id = $3)
INSERT INTO variant_weights(environment_id, variant_id, segment_id, accumulator, weight)
SELECT
  $1,
  v.variant_id,
  $3,
  calc.remaining_weight,
  calc.remaining_weight
FROM variants v
JOIN calc USING(environment_id)
WHERE v.environment_id = $1 AND v.feature_id = $2
ON CONFLICT(variant_id, environment_id, segment_id) WHERE segment_id IS NOT NULL
DO UPDATE SET accumulator = excluded.accumulator, weight = excluded.weight
RETURNING variant_id, weight

-- :name update_variant_value :|| :1
-- :doc Updates value of given feature variant
UPDATE variants SET value = $2
WHERE environment_id IS NULL AND variant_id = $1
RETURNING feature_id

-- :name update_variant_accumulator :<> :!
-- :doc Updates accumulator of given feature variant, scoped to a segment (NULL = organic)
UPDATE variant_weights SET accumulator = $4
WHERE environment_id = $1 AND variant_id = $2 AND segment_id IS $3

-- :name fetch_variant_by_id :<> :1
-- :doc Fetches a variant of given id in given environment, scoped to a segment (NULL = organic)
SELECT v.variant_id, v.environment_id, feature_id, value, COALESCE(weight, 0) AS weight, COALESCE(accumulator, 0) AS accumulator
FROM variants v
LEFT JOIN variant_weights vw ON v.variant_id = vw.variant_id AND vw.environment_id = $1 AND vw.segment_id IS $3
WHERE v.variant_id = $2

-- :name fetch_variant_by_value :<> :?
-- :doc Fetches a variant of given value (control or not) in given environment, scoped to a segment (NULL = organic)
SELECT v.variant_id, v.environment_id, feature_id, value, COALESCE(weight, 0) AS weight, COALESCE(accumulator, 0) AS accumulator
FROM variants v
LEFT JOIN variant_weights vw ON v.variant_id = vw.variant_id AND vw.environment_id = $1 AND vw.segment_id IS $4
WHERE v.feature_id = $2 AND v.value = $3 AND COALESCE(v.environment_id, $1) = $1

-- :name fetch_variants_for_feature :<> :*
-- :doc Fetches all variants for given feature, scoped to a segment's weights (NULL = organic default weights)
SELECT v.variant_id, v.environment_id, v.value, COALESCE(vw.weight, 0) AS weight, COALESCE(vw.accumulator, 0) AS accumulator
FROM variants v
LEFT JOIN variant_weights vw ON vw.variant_id = v.variant_id AND vw.environment_id = $1 AND vw.segment_id IS $3
WHERE feature_id = $2 AND COALESCE(v.environment_id, $1) = $1
ORDER BY weight DESC

-- :name delete_segment_variant_weights_for_feature :<> :!
-- :doc Removes all segment-scoped weight overrides for a segment+feature+environment
DELETE FROM variant_weights
WHERE segment_id = $1 AND environment_id = $3
  AND variant_id IN (SELECT variant_id FROM variants WHERE feature_id = $2)

-- :name fetch_segment_overrides_with_weights :<> :*
-- :doc Returns (segment_id, segment_name, variant_id, weight) for all segments overriding
-- feature+environment. Includes the control variant's auto-balanced remainder row (listed
-- first per segment) so callers can display where the rest of the percentages go, or (for
-- the rule evaluator) run a full weighted distribution once a segment matches. Ordered by
-- segment_id ascending (creation order) so callers grouping these rows don't need to re-sort.
SELECT vw.segment_id, s.name, vw.variant_id, vw.weight
FROM variant_weights vw
JOIN segments s USING(segment_id)
JOIN variants v ON v.variant_id = vw.variant_id
WHERE v.feature_id = $1 AND vw.environment_id = $2 AND vw.segment_id IS NOT NULL
ORDER BY vw.segment_id, (v.environment_id IS NULL), vw.variant_id

-- :name fetch_segment_variant_weights :<> :*
-- :doc Returns variant_id + weight overrides for a given segment+feature+environment.
-- Excludes the control variant's auto-balanced remainder row - only explicit overrides are shown.
SELECT vw.variant_id, vw.weight
FROM variant_weights vw
JOIN variants v ON v.variant_id = vw.variant_id
WHERE vw.segment_id = $1 AND v.feature_id = $2 AND vw.environment_id = $3 AND v.environment_id IS NULL

-- :name fetch_features_overridden_by_segment :<> :*
-- :doc Returns (feature_id, feature_name, variant_id, is_control, value, weight) for every
-- variant this segment overrides (including each feature's control-variant remainder),
-- across all features, within a given environment.
SELECT f.feature_id, f.name AS feature_name, vw.variant_id,
       (v.environment_id IS NOT NULL) AS is_control, v.value, vw.weight
FROM variant_weights vw
JOIN variants v ON v.variant_id = vw.variant_id
JOIN features f ON f.feature_id = v.feature_id
WHERE vw.segment_id = $1 AND vw.environment_id = $2
ORDER BY f.name, (v.environment_id IS NULL), vw.variant_id

-- :name fetch_variants_for_identity :<> :*
-- :doc Fetches feature variants for given identity. Variants attached to identity by distributor are denoted by non-NULL identity_id field.
SELECT f.feature_id, iv.variant_id, f.name AS feature_name, iv_v.value AS feature_value, iv.migrated_id, iv.pinned_at, iv.identity_id
FROM features f
LEFT JOIN identities i ON i.identity = lower($3) AND i.environment_id = $2
LEFT JOIN identity_variants iv ON iv.feature_id = f.feature_id AND iv.environment_id = $2 AND iv.identity_id = i.identity_id
LEFT JOIN variants iv_v ON iv_v.variant_id = iv.variant_id
WHERE f.archived_at IS NULL AND f.project_id = $1
ORDER BY iv.identity_id DESC

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
