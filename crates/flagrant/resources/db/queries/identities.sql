-- :name upsert_identity :<> :1
-- :doc Connects identity with variant of given id
INSERT INTO identities(identity)
VALUES(lower($1))
ON CONFLICT (identity) DO UPDATE SET updated_at = CURRENT_TIMESTAMP
RETURNING identity_id, identity

-- :name upsert_identity_variant :<> :!
-- :doc Connects identity with variant of given id
INSERT INTO identity_variants(identity_id, environment_id, feature_id, variant_id)
VALUES($1, $2, $3, $4)
ON CONFLICT(identity_id, feature_id, environment_id) DO UPDATE SET variant_id = excluded.variant_id, migrated_id = NULL

-- :name fetch_identities :<> :*
-- :doc Returns all identities attached to given feature
SELECT iv.identity_id, iv.feature_id, iv.variant_id, iv.environment_id, iv.migrated_id, i.identity
FROM identity_variants iv JOIN identities i USING(identity_id)
WHERE environment_id = $1 AND feature_id = $2

-- :name migrate_identities :<> :!
-- :doc Migrates number of identities attached to one variant to the other one by given percent
WITH attached AS (
  SELECT identity_id, migrated_id, attached_at
  FROM identity_variants
  WHERE environment_id = $1 AND ((variant_id = $2 AND migrated_id IS NULL) OR migrated_id = $2)
)
UPDATE identity_variants SET migrated_id = $3
WHERE environment_id = $1 AND identity_id IN (
  SELECT identity_id FROM attached
  ORDER BY migrated_id DESC, attached_at
  LIMIT (
    -- round division up
    SELECT MAX(0, (SELECT CAST((COUNT(*) * $4 + 99) / 100.0 AS INTEGER) FROM identities))
  )
)
