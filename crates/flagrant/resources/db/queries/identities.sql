-- :name upsert_identity :<> :1
-- :doc Connects identity with variant of given id
INSERT INTO identities(identity)
VALUES(lower($1))
ON CONFLICT (identity) DO UPDATE SET last_seen_at = CURRENT_TIMESTAMP
RETURNING identity_id, identity

-- :name upsert_identity_variant :<> :!
-- :doc Connects identity with variant of given id
INSERT INTO identities_variants(identity_id, feature_id, variant_id)
VALUES($1, $2, $3)
ON CONFLICT(identity_id, feature_id) DO UPDATE SET variant_id = excluded.variant_id, detached_at = NULL

