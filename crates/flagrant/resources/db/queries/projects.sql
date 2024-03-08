-- :name create_project :<> :1
-- :doc Creates a new project with a name
INSERT INTO projects(name) VALUES($1)
RETURNING project_id, name

-- :name fetch_project :<> :1
-- :doc Fetches a project of given id
SELECT project_id, name
FROM projects
WHERE project_id = $1
