-- :name fetch_tags_by_pattern  :|| :*
-- :doc Returns a list of tags starting with given prefix
SELECT tag
FROM feature_tags ft
JOIN features f USING (feature_id)
JOIN projects p ON f.project_id = p.project_id
WHERE p.project_id = $1 AND tag LIKE $2
ORDER BY tag
