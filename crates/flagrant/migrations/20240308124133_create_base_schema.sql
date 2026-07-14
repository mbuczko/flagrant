CREATE TABLE IF NOT EXISTS projects (
  project_id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

  UNIQUE(name)
);

CREATE TABLE IF NOT EXISTS environments (
  environment_id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id INTEGER NOT NULL REFERENCES projects,
  name TEXT NOT NULL CHECK(LENGTH(name) <= 255),
  description TEXT CHECK(LENGTH(name) <= 2048),
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

  UNIQUE(project_id, name)
);

CREATE TABLE IF NOT EXISTS features (
  feature_id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id INTEGER NOT NULL REFERENCES projects,
  name TEXT NOT NULL CHECK(LENGTH(name) <= 255),
  description TEXT CHECK(LENGTH(description) <= 2048),
  is_enabled BOOLEAN NOT NULL DEFAULT FALSE,
  archived_at DATETIME,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  version INTEGER NOT NULL DEFAULT 0,

  UNIQUE(project_id, name)
);

CREATE TABLE IF NOT EXISTS feature_tags (
  feature_id INTEGER NOT NULL REFERENCES features,
  tag TEXT NOT NULL CHECK(LENGTH(tag) <= 32)
);

CREATE TABLE IF NOT EXISTS variants (
  variant_id INTEGER PRIMARY KEY AUTOINCREMENT,
  feature_id INTEGER NOT NULL REFERENCES features,
  -- environment_id is set only for control value
  environment_id INTEGER REFERENCES environments,
  value TEXT NOT NULL CHECK(LENGTH(value) <= 1024),
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  version INTEGER NOT NULL DEFAULT 0,

  UNIQUE(feature_id, variant_id),
  UNIQUE(feature_id, environment_id)
);

-- using a partial index, ensure that there is only one control value for feature per environment
--CREATE UNIQUE INDEX idx_unique_is_control ON variants(feature_id, environment_id) WHERE is_control = true;

-- Enforce unique values only among non-control variants (environment_id IS NULL).
-- Control variants may share the same value across different environments.
CREATE UNIQUE INDEX IF NOT EXISTS idx_unique_variant_value ON variants(feature_id, value) WHERE environment_id IS NULL;

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

-- segment_id NULL rows are the organic/default weights; segment_id-scoped rows are a
-- per-segment weight table (each with its own accumulator) used to override which
-- variant a segment's identities are distributed across.
CREATE TABLE IF NOT EXISTS variant_weights (
  variant_id INTEGER NOT NULL REFERENCES variants,
  environment_id INTEGER NOT NULL REFERENCES environments,
  segment_id INTEGER REFERENCES segments ON DELETE CASCADE,
  weight INTEGER NOT NULL DEFAULT 0 CHECK (weight >= 0 and weight <= 100),
  accumulator INTEGER NOT NULL DEFAULT 100
);

-- exactly one default (organic) row per variant+environment
CREATE UNIQUE INDEX IF NOT EXISTS idx_variant_weights_default
  ON variant_weights(variant_id, environment_id) WHERE segment_id IS NULL;

-- exactly one row per variant+environment+segment
CREATE UNIQUE INDEX IF NOT EXISTS idx_variant_weights_segment
  ON variant_weights(variant_id, environment_id, segment_id) WHERE segment_id IS NOT NULL;

-- Plain (non-partial) covering index for lookups. Every read/update against this table
-- filters on all three columns via a bound parameter (`segment_id IS ?`), and SQLite
-- cannot use a partial index when matching its WHERE clause depends on a bound
-- parameter's runtime value rather than a literal NULL/NOT NULL - so without this index,
-- those queries fall back to a full table scan on every single feature-flag evaluation.
CREATE INDEX IF NOT EXISTS idx_variant_weights_lookup
  ON variant_weights(variant_id, environment_id, segment_id);

CREATE TABLE IF NOT EXISTS traits (
  trait_id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id INTEGER NOT NULL REFERENCES projects,
  name TEXT NOT NULL CHECK(LENGTH(name) <= 255),
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

  UNIQUE(project_id, name)
);

CREATE TABLE IF NOT EXISTS identities (
  identity_id INTEGER PRIMARY KEY AUTOINCREMENT,
  environment_id INTEGER NOT NULL REFERENCES environments,
  identity TEXT NOT NULL CHECK(LENGTH(identity) <= 255),
  updated_at DATETIME,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

  UNIQUE(environment_id, identity)
);

CREATE TABLE IF NOT EXISTS identity_traits (
  identity_id INTEGER NOT NULL REFERENCES identities,
  trait_id INTEGER NOT NULL REFERENCES traits,
  value TEXT CHECK (LENGTH(value) <= 1024),

  PRIMARY KEY (identity_id, trait_id)
);


CREATE TABLE IF NOT EXISTS identity_variants (
  identity_id INTEGER NOT NULL REFERENCES identities,
  feature_id INTEGER NOT NULL REFERENCES features,
  variant_id INTEGER NOT NULL REFERENCES variants,
  environment_id INTEGER NOT NULL REFERENCES environments,
  migrated_id INTEGER REFERENCES variants,
  attached_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  pinned_at DATETIME,

  UNIQUE(identity_id, feature_id, environment_id),

  FOREIGN KEY (feature_id, variant_id) REFERENCES variants(feature_id, variant_id)
);
