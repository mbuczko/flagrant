-- :name upsert_identity :<> :1
-- :doc Connects identity with variant of given id
INSERT INTO identities(identity)
VALUES(lower($1))
ON CONFLICT (identity) DO UPDATE SET updated_at = CURRENT_TIMESTAMP
RETURNING identity_id, identity

-- :name upsert_identity_variant :<> :!
-- :doc Connects identity with variant of given id
INSERT INTO identity_variants(identity_id, environment_id, feature_id, variant_id)
VALUES($1, $2, $3, $3)
ON CONFLICT(identity_id, feature_id, environment_id) DO UPDATE SET variant_id = excluded.variant_id, detached_at = NULL

-- :name fetch_identities_count :|| :1
-- :doc Returns number of registered identities
SELECT count(1) as "count" FROM identities

-- :name detach_identities_from_variant :<> :!
-- :doc Marks identity as detached from a feature variant in given environment
UPDATE identity_variants SET detatched_at = CURRENT_TIMESTAMP
WHERE environment_id = $1
ORDER BY attached_at DESC
LIMIT $2
