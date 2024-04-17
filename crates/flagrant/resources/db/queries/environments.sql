-- :name create_environment :<> :1
-- :doc Creates a new environment with name and description
INSERT INTO environments(project_id, name, description) VALUES($1, $2, $3)
RETURNING environment_id, project_id, name, description

-- :name fetch_environment :<> :1
-- :doc Returns a environment of given id
SELECT environment_id, project_id, name, description
FROM environments
WHERE environment_id = $1

-- :name fetch_environments_for_project :<> :*
-- :doc Returns all environments of given project
SELECT environment_id, project_id, name, description
FROM environments
WHERE project_id = $1

-- :name fetch_environment_by_name :<> :1
-- :doc Returns environment of given name
SELECT environment_id, project_id, name, description
FROM environments
WHERE project_id = $1 AND name = $2

-- :name fetch_environments_by_pattern :<> :*
-- :doc Returns list of environments in a project with names matching given pattern
SELECT environment_id, project_id, name, description
FROM environments
WHERE project_id = $1 AND name LIKE $2
