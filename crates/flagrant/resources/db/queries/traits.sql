-- :name upsert_trait :<> :1
-- :doc Creates or returns existing trait by name
INSERT INTO traits(project_id, name) VALUES($1, $2)
ON CONFLICT(project_id, name) DO UPDATE SET name = excluded.name
RETURNING trait_id, name

-- :name fetch_trait_by_id :<> :1
-- :doc Returns trait by its id
SELECT trait_id, name FROM traits t
WHERE t.project_id = $1 AND t.trait_id = $2

-- :name fetch_all_traits :<> :*
-- :doc Returns all traits ordered by name
SELECT trait_id, name FROM traits t
WHERE t.project_id = $1
ORDER BY name

-- :name delete_trait_entries :<> :!
-- :doc Removes all identity_traits entries for given trait
DELETE FROM identity_traits WHERE trait_id = $1

-- :name delete_trait :<> :!
-- :doc Removes the trait itself
DELETE FROM traits WHERE trait_id = $1
