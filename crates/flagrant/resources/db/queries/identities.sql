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
ON CONFLICT(identity_id, feature_id, environment_id) DO UPDATE SET variant_id = excluded.variant_id, detached_at = NULL

-- :name reset_detached_identities :<> :!
-- :doc Unmarks 'detached' identities for given variant
UPDATE identity_variants SET detached_at = NULL
WHERE environment_id = $1 AND variant_id = $2

-- :name detach_identities_from_variant :<> :!
-- :doc Marks identity as detached from a feature variant in given environment
WITH attached AS (
  SELECT identity_id, attached_at
  FROM identity_variants
  WHERE environment_id = $1 AND variant_id = $2
)
UPDATE identity_variants SET detached_at = CURRENT_TIMESTAMP
WHERE environment_id = $1 AND variant_id = $2 AND identity_id IN (
  SELECT identity_id FROM attached ORDER BY attached_at
  LIMIT (
    SELECT MAX(0, COUNT(*) - (SELECT CAST((COUNT(*) * $3) / 100 AS INTEGER) FROM identities))
    FROM attached
  )
)
