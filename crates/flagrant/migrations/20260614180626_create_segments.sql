CREATE TABLE IF NOT EXISTS segments (
  segment_id  INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id  INTEGER NOT NULL REFERENCES projects,
  name        TEXT NOT NULL CHECK(LENGTH(name) <= 255),
  description TEXT CHECK(LENGTH(description) <= 2048),
  created_at  DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

  UNIQUE(project_id, name)
);

CREATE TABLE IF NOT EXISTS segment_groups (
  group_id    INTEGER PRIMARY KEY AUTOINCREMENT,
  segment_id  INTEGER NOT NULL REFERENCES segments ON DELETE CASCADE,
  position    INTEGER NOT NULL,
  label       TEXT NOT NULL,
  connector   TEXT,
  description TEXT CHECK(LENGTH(description) <= 2048),

  UNIQUE(segment_id, position),
  UNIQUE(segment_id, label)
);

CREATE TABLE IF NOT EXISTS segment_rules (
  rule_id     INTEGER PRIMARY KEY AUTOINCREMENT,
  group_id    INTEGER NOT NULL REFERENCES segment_groups ON DELETE CASCADE,
  driver      TEXT NOT NULL,
  comparator  TEXT NOT NULL,
  value       TEXT NOT NULL CHECK(LENGTH(value) <= 1024)
);
