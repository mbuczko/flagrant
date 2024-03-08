-- :name create_environment :<> :1
-- :doc Creates a new environment with name and description
INSERT INTO environments(project_id, name, description) VALUES($1, $2, $3)
RETURNING environment_id, name, description

-- :name fetch_environment :<> :1
-- :doc Fetches a environment of given id
SELECT environment_id, name, description
FROM environments
WHERE environment_id = $1
