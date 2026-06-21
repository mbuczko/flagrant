-- :name create_segment :<> :1
-- :doc Creates a new segment with name and optional description
INSERT INTO segments(project_id, name, description) VALUES($1, $2, $3)
RETURNING segment_id, project_id, name, description

-- :name fetch_segment_by_id :<> :1
-- :doc Returns a segment row for the given segment_id and project_id
SELECT segment_id, project_id, name, description
FROM segments
WHERE segment_id = $1 AND project_id = $2

-- :name fetch_segment_by_name :<> :1
-- :doc Returns a segment row for the given name and project_id
SELECT segment_id, project_id, name, description
FROM segments
WHERE name = $1 AND project_id = $2

-- :name fetch_segments :<> :*
-- :doc Returns all segments for the given project
SELECT segment_id, project_id, name, description
FROM segments
WHERE project_id = $1
ORDER BY name

-- :name fetch_segments_by_pattern :<> :*
-- :doc Returns segments for the given project with names matching a LIKE pattern
SELECT segment_id, project_id, name, description
FROM segments
WHERE project_id = $1 AND name LIKE $2
ORDER BY name

-- :name update_segment :<> :!
-- :doc Updates segment name and description
UPDATE segments SET name = $2, description = $3 WHERE segment_id = $1

-- :name delete_segment :<> :!
-- :doc Deletes a segment by id
DELETE FROM segments WHERE segment_id = $1

-- :name add_group :<> :1
-- :doc Inserts a new group. $1=segment_id, $2=position, $3=label, $4=connector, $5=description
INSERT INTO segment_groups(segment_id, position, label, connector, description)
VALUES($1, $2, $3, $4, $5)
RETURNING group_id, segment_id, position, label, connector, description

-- :name delete_group :<> :!
-- :doc Deletes a group by id (rules are cascade-deleted)
DELETE FROM segment_groups WHERE group_id = $1

-- :name clear_initial_group_connector :<> :!
-- :doc Sets connector to NULL for the group with the lowest position (new head after deletion)
UPDATE segment_groups SET connector = NULL
WHERE group_id = (SELECT group_id FROM segment_groups WHERE segment_id = $1 ORDER BY position LIMIT 1)

-- :name add_rule :<> :1
-- :doc Inserts a new rule into a group
INSERT INTO segment_rules(group_id, driver, comparator, value) VALUES($1, $2, $3, $4)
RETURNING rule_id, driver, comparator, value

-- :name fetch_rules_for_group :<> :*
-- :doc Returns all rules for a group ordered by rule_id
SELECT rule_id, driver, comparator, value
FROM segment_rules
WHERE group_id = $1
ORDER BY rule_id

-- :name fetch_rules_for_segment :<> :*
-- :doc Returns all rules for all groups of a segment (includes group_id for assembly)
SELECT r.rule_id, r.group_id, r.driver, r.comparator, r.value
FROM segment_rules r
JOIN segment_groups g ON g.group_id = r.group_id
WHERE g.segment_id = $1
ORDER BY g.position, r.rule_id

-- :name fetch_groups_for_segment :<> :*
-- :doc Returns all groups for a segment ordered by position
SELECT group_id, segment_id, position, label, connector, description
FROM segment_groups
WHERE segment_id = $1
ORDER BY position

-- :name fetch_groups_for_segments :<> :*
-- :doc Returns all groups for segments in a project (all or specific one if id was provided)
SELECT g.group_id, g.segment_id, g.position, g.label, g.connector, g.description
FROM segment_groups g
JOIN segments s ON s.segment_id = g.segment_id
WHERE s.project_id = $1
ORDER BY g.segment_id, g.position

-- :name fetch_rules :<> :*
-- :doc Returns all rules for all segments in a project (includes group_id for assembly)
SELECT r.rule_id, r.group_id, r.driver, r.comparator, r.value
FROM segment_rules r
JOIN segment_groups g ON g.group_id = r.group_id
JOIN segments s ON s.segment_id = g.segment_id
WHERE s.project_id = $1
ORDER BY g.segment_id, g.position, r.rule_id

-- :name delete_rule :<> :!
-- :doc Deletes a rule by id
DELETE FROM segment_rules WHERE rule_id = $1
