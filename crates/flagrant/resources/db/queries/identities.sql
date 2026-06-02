-- :name fetch_identity_by_id :<> :1
-- :doc Fetches a single identity by id
SELECT identity_id, identity, project_id FROM identities WHERE project_id = $1 AND identity_id = $2

-- :name fetch_identity_by_value :<> :1
-- :doc Fetches a single identity by project and value
SELECT identity_id, identity, project_id FROM identities WHERE project_id = $1 AND identity = lower($2)

-- :name fetch_identities_with_traits :<> :*
-- :doc Lists up to 10 identities with their traits matching LIKE pattern (use '%' to match all)
SELECT i.identity_id, i.identity, t.trait_id, t.name AS trait_name, it.value AS trait_value
FROM (
    SELECT project_id, identity_id, identity FROM identities
    WHERE  project_id = $1 and identity LIKE $2
    ORDER BY identity
    LIMIT 10
) i
LEFT JOIN identity_traits it USING(identity_id)
LEFT JOIN traits t ON t.trait_id = it.trait_id AND t.project_id = i.project_id
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

-- :name update_identity :<> :!
-- :doc Updates identity value
UPDATE identities SET identity = lower($1) WHERE identity_id = $2

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

-- :name upsert_identity :<> :1
-- :doc Connects identity with variant of given id
INSERT INTO identities(project_id, identity)
VALUES($1, lower($2))
ON CONFLICT (project_id, identity) DO UPDATE SET updated_at = CURRENT_TIMESTAMP
RETURNING identity_id, identity, project_id

-- :name fetch_identity_variant_for_feature :<> :?
-- :doc Returns variant_id assigned to identity for given feature+environment
SELECT iv.variant_id FROM identity_variants iv
WHERE iv.identity_id = $1 AND iv.feature_id = $2 AND iv.environment_id = $3

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
-- :doc Migrates given percent of identities attached to one variant into the other variant
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
-- :name delete_attachments :<> :!
-- :doc Removes attachments of all identitites to given variant. This is executed only on variant deletion.
DELETE FROM identity_variants WHERE variant_id = $1 OR migrated_id = $1
