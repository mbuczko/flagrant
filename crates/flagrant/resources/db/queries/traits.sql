-- :name upsert_trait :<> :1
-- :doc Creates or returns existing trait by name
INSERT INTO traits(name) VALUES($1)
ON CONFLICT(name) DO UPDATE SET name = excluded.name
RETURNING trait_id, name

-- :name fetch_all_traits :<> :*
-- :doc Returns all traits ordered by name
SELECT trait_id, name FROM traits ORDER BY name

-- :name delete_trait_entries :<> :!
-- :doc Removes all identity_traits entries for given trait
DELETE FROM identity_traits WHERE trait_id = $1

-- :name delete_trait :<> :!
-- :doc Removes the trait itself
DELETE FROM traits WHERE trait_id = $1
