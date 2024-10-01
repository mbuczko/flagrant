-- :name upsert_identity :<> :!
-- :doc Connects identity with variant of given id
INSERT INTO identities(identity, variant_id)
VALUES($1, $2)
ON CONFLICT(identity) DO UPDATE SET variant_id = excluded.variant_id, detached_at = NULL
