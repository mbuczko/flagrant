CREATE TABLE IF NOT EXISTS projects (
  project_id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS environments (
  environment_id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id INTEGER NOT NULL REFERENCES projects,
  name TEXT NOT NULL,
  description TEXT,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS features (
  feature_id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id INTEGER NOT NULL REFERENCES projects,
  name TEXT NOT NULL CHECK(LENGTH(name) <= 255),
  description TEXT CHECK(LENGTH(description) <= 2048),
  is_enabled BOOLEAN NOT NULL DEFAULT FALSE,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  version INTEGER NOT NULL DEFAULT 0,

  UNIQUE(project_id, name)
);

CREATE TABLE IF NOT EXISTS variants (
  variant_id INTEGER PRIMARY KEY AUTOINCREMENT,
  feature_id INTEGER NOT NULL REFERENCES features,
  -- environment_id is set only for control value
  environment_id INTEGER REFERENCES environments,
  value TEXT NOT NULL,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
  version INTEGER NOT NULL DEFAULT 0,

  UNIQUE(feature_id, variant_id),
  UNIQUE(feature_id, environment_id)
);

-- using a partial index, ensure that there is only one control value for feature per environment
--CREATE UNIQUE INDEX idx_unique_is_control ON variants(feature_id, environment_id) WHERE is_control = true;

CREATE TABLE IF NOT EXISTS variant_weights (
  variant_id INTEGER NOT NULL REFERENCES variants,
  environment_id INTEGER NOT NULL REFERENCES environments,
  weight INTEGER NOT NULL DEFAULT 0 CHECK (weight >= 0 and weight <= 100),
  accumulator INTEGER NOT NULL DEFAULT 100,
  PRIMARY KEY (variant_id, environment_id)
);

CREATE TABLE IF NOT EXISTS identities (
  identity_id INTEGER PRIMARY KEY AUTOINCREMENT,
  identity TEXT NOT NULL UNIQUE,
  updated_at DATETIME,
  created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS identity_variants (
  identity_id INTEGER NOT NULL REFERENCES identities,
  feature_id INTEGER NOT NULL REFERENCES features,
  variant_id INTEGER NOT NULL REFERENCES variants,
  environment_id INTEGER NOT NULL REFERENCES environments,
  migrated_id INTEGER REFERENCES variants,
  attached_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,

  UNIQUE(identity_id, feature_id, environment_id),

  FOREIGN KEY (feature_id, variant_id) REFERENCES variants(feature_id, variant_id),
  FOREIGN KEY (variant_id, environment_id) REFERENCES variant_weights(variant_id, environment_id)
);
