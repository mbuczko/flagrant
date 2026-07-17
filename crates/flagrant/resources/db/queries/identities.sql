-- :name fetch_identity_by_id :<> :1
-- :doc Fetches a single identity by id
SELECT identity_id, identity, environment_id FROM identities WHERE environment_id = $1 AND identity_id = $2

-- :name fetch_identity_by_value :<> :1
-- :doc Fetches a single identity by environment and value
SELECT identity_id, identity, environment_id FROM identities WHERE environment_id = $1 AND identity = lower($2)

-- :name fetch_identities_with_traits :<> :*
-- :doc Lists up to 10 identities with their traits matching LIKE pattern (use '%' to match all)
SELECT i.identity_id, i.identity, t.trait_id, t.name AS trait_name, it.value AS trait_value
FROM (
    SELECT identity_id, identity FROM identities id
    WHERE id.environment_id = $2 AND id.identity LIKE $3
--~{ traits_included
    AND EXISTS (
      SELECT 1 FROM identity_traits it2, traits t2, json_each($4) je
      WHERE it2.identity_id = id.identity_id AND it2.trait_id = t2.trait_id
        AND t2.project_id = $1 AND t2.name = json_extract(je.value, '$[0]')
        AND (
          json_extract(je.value, '$[1]') IS NULL
          OR it2.value IN (SELECT value FROM json_each(json_extract(je.value, '$[1]')))
        )
    )
--~}
--~{ traits_excluded
    AND NOT EXISTS (
      SELECT 1 FROM identity_traits it2, traits t2, json_each($5) je
      WHERE it2.identity_id = id.identity_id AND it2.trait_id = t2.trait_id
        AND t2.project_id = $1 AND t2.name = json_extract(je.value, '$[0]')
        AND (
          json_extract(je.value, '$[1]') IS NULL
          OR it2.value IN (SELECT value FROM json_each(json_extract(je.value, '$[1]')))
        )
    )
--~}
    ORDER BY identity
    LIMIT 10
) i
LEFT JOIN identity_traits it USING(identity_id)
LEFT JOIN traits t ON t.trait_id = it.trait_id AND t.project_id = $1
ORDER BY i.identity, t.name

-- :name fetch_identity_traits :<> :*
-- :doc Fetches all traits attached to given identity
SELECT t.trait_id, t.name, it.value
FROM identity_traits it
JOIN traits t USING(trait_id)
WHERE it.identity_id = $1
ORDER BY t.name

-- :name upsert_identity_trait :<> :!
-- :doc Upserts a trait value for given identity
INSERT INTO identity_traits(identity_id, trait_id, value)
VALUES($1, $2, $3)
ON CONFLICT(identity_id, trait_id) DO UPDATE SET value = excluded.value

-- :name delete_identity_traits :<> :!
-- :doc Removes all trait entries for given identity
DELETE FROM identity_traits WHERE identity_id = $1

-- :name delete_identity_trait_by_name :<> :!
-- :doc Removes a single trait from an identity, looked up by name
DELETE FROM identity_traits
WHERE identity_id = $1
  AND trait_id = (SELECT trait_id FROM traits WHERE project_id = $2 AND name = $3)

-- :name delete_identity_variants :<> :!
-- :doc Removes all variant assignments for given identity
DELETE FROM identity_variants WHERE identity_id = $1

-- :name delete_identity :<> :!
-- :doc Removes an identity record
DELETE FROM identities WHERE identity_id = $1

-- :name delete_identity_traits_for_environment_pattern :<> :!
-- :doc Removes trait entries for every identity in an environment matching a LIKE pattern
DELETE FROM identity_traits
WHERE identity_id IN (
    SELECT identity_id FROM identities WHERE environment_id = $1 AND identity LIKE $2
)

-- :name delete_identity_variants_for_environment_pattern :<> :!
-- :doc Removes variant assignments for every identity in an environment matching a LIKE pattern
DELETE FROM identity_variants
WHERE identity_id IN (
    SELECT identity_id FROM identities WHERE environment_id = $1 AND identity LIKE $2
)

-- :name delete_identity_variants_for_feature_pattern :<> :!
-- :doc Removes variant assignments for a single feature, for every identity in an environment matching a LIKE pattern
DELETE FROM identity_variants
WHERE feature_id = $1 AND environment_id = $2 AND identity_id IN (
    SELECT identity_id FROM identities WHERE environment_id = $2 AND identity LIKE $3
)

-- :name delete_identities_for_environment_pattern :<> :!
-- :doc Removes identity records in an environment matching a LIKE pattern
DELETE FROM identities WHERE environment_id = $1 AND identity LIKE $2

-- :name upsert_identity :<> :1
-- :doc Creates or updates an identity scoped to the given environment
INSERT INTO identities(environment_id, identity)
VALUES($1, lower($2))
ON CONFLICT (environment_id, identity) DO UPDATE SET updated_at = CURRENT_TIMESTAMP
RETURNING identity_id, identity, environment_id

-- :name fetch_identity_variant_for_feature :<> :?
-- :doc Returns variant_id assigned to identity for given feature+environment
SELECT iv.variant_id FROM identity_variants iv
WHERE iv.identity_id = $1 AND iv.feature_id = $2 AND iv.environment_id = $3

-- :name upsert_identity_variant :<> :!
-- :doc Connects identity with variant of given id, attributed to segment_id (NULL = organic
-- or an explicit pin). Pass a non-NULL pinned_at to mark as an explicit override. Always
-- clears segment_dirty - this is the write side of resolving a dirty row.
INSERT INTO identity_variants(identity_id, environment_id, feature_id, variant_id, segment_id, pinned_at)
VALUES($1, $2, $3, $4, $5, $6)
ON CONFLICT(identity_id, feature_id, environment_id) DO UPDATE SET
  variant_id = excluded.variant_id, segment_id = excluded.segment_id,
  migrated_id = NULL, segment_dirty = FALSE, pinned_at = excluded.pinned_at

-- :name clear_identity_dirty :<> :!
-- :doc Clears segment_dirty for a single identity+feature+environment row without touching
-- variant_id/segment_id - used when a dirty row is read and re-evaluated but its segment
-- attribution turns out unchanged, so no redistribution (and no accumulator perturbation)
-- is needed, just acknowledging the row has been checked.
UPDATE identity_variants SET segment_dirty = FALSE
WHERE identity_id = $1 AND feature_id = $2 AND environment_id = $3

-- :name mark_feature_dirty :<> :!
-- :doc Flags every already-distributed, unpinned identity for a feature+environment as
-- needing re-evaluation against current segment state. Cheap and unconditional (cost is
-- independent of how many rows match) - resolution itself is deferred to the next time
-- each flagged identity is actually read, via get_identity_variants.
UPDATE identity_variants SET segment_dirty = TRUE
WHERE feature_id = $1 AND environment_id = $2 AND pinned_at IS NULL

-- :name fetch_identities :<> :*
-- :doc Returns all identities attached to given feature
SELECT iv.identity_id, iv.feature_id, iv.variant_id, iv.environment_id, iv.migrated_id,
       iv.segment_id, iv.segment_dirty, i.identity
FROM identity_variants iv JOIN identities i USING(identity_id)
WHERE iv.environment_id = $1 AND iv.feature_id = $2

-- :name fetch_overrides_for_feature :<> :*
-- :doc Returns identity values that have an explicit override (pinned_at IS NOT NULL) for given feature
SELECT i.identity
FROM identity_variants iv JOIN identities i USING(identity_id)
WHERE iv.environment_id = $1 AND iv.feature_id = $2 AND iv.pinned_at IS NOT NULL
ORDER BY i.identity

-- :name migrate_identities :<> :!
-- :doc Migrates given percent of organic (non-segment-governed) identities attached to one
-- variant into the other variant, for the feature those variants belong to. Segment-governed
-- identities (segment_id IS NOT NULL) are left untouched - their assignment is reconciled
-- separately, lazily, via the segment_dirty flag (see mark_feature_dirty).
WITH attached AS (
  SELECT identity_id, migrated_id, attached_at
  FROM identity_variants
  WHERE environment_id = $1 AND ((variant_id = $2 AND migrated_id IS NULL) OR migrated_id = $2)
    AND pinned_at IS NULL AND segment_id IS NULL
)
UPDATE identity_variants SET migrated_id = $3
WHERE environment_id = $1
  AND feature_id = (SELECT feature_id FROM variants WHERE variant_id = $2)
  AND identity_id IN (
    SELECT identity_id FROM attached
    ORDER BY migrated_id DESC, attached_at
    LIMIT (
      -- round division up
      SELECT MAX(0, (SELECT CAST((COUNT(*) * $4 + 99) / 100.0 AS INTEGER) FROM identities WHERE environment_id = $1))
    )
  )

-- :name delete_identity_variant_for_feature :<> :!
-- :doc Removes a single variant assignment for given identity+feature+environment (unpin)
DELETE FROM identity_variants WHERE identity_id = $1 AND feature_id = $2 AND environment_id = $3

-- :name delete_attachments :<> :!
-- :doc Removes attachments of all identitites to given variant. This is executed only on variant deletion.
DELETE FROM identity_variants WHERE variant_id = $1 OR migrated_id = $1
